#!/usr/bin/env python3
"""Generate Rust source with parser-backed card registry entries from cards.json."""

from __future__ import annotations

import argparse
import json
import os
from pathlib import Path
from typing import Dict, List, Tuple

from stream_scryfall_blocks import build_block, is_non_playable, iter_json_array


ROOT = Path(__file__).resolve().parents[1]
CARDS_JSON = ROOT / "cards.json"
OUT_FILE = ROOT / "src" / "cards" / "generated_registry.rs"

DIGITAL_ONLY_ORACLE_MARKERS = (
    "boon",
    "conjure",
    "double team",
    "draft",
    "heist",
    "incorporate",
    "intensity",
    "intensify",
    "perpetually",
    "seek",
    "specialize",
    "spellbook",
)


def contains_marker_with_boundaries(text: str, marker: str) -> bool:
    start = text.find(marker)
    while start != -1:
        before = text[start - 1] if start > 0 else ""
        after_index = start + len(marker)
        after = text[after_index] if after_index < len(text) else ""
        before_is_letter = before.isalpha()
        after_is_letter = after.isalpha()
        if not before_is_letter and not after_is_letter:
            return True
        start = text.find(marker, start + 1)
    return False


def has_digital_only_oracle_marker(oracle_text: str) -> bool:
    lower = oracle_text.lower()
    return any(
        contains_marker_with_boundaries(lower, marker)
        for marker in DIGITAL_ONLY_ORACLE_MARKERS
    )


def card_oracle_text(card: dict) -> str | None:
    oracle_text = card.get("oracle_text")
    if isinstance(oracle_text, str):
        return oracle_text
    faces = card.get("card_faces")
    if isinstance(faces, list) and faces:
        first = faces[0]
        if isinstance(first, dict):
            face_oracle = first.get("oracle_text")
            if isinstance(face_oracle, str):
                return face_oracle
    return None


def rust_raw_literal(value: str) -> str:
    """Return a Rust string literal (raw when possible)."""
    for hashes in range(0, 12):
        marks = "#" * hashes
        end = f'"{marks}'
        if end not in value:
            return f'r{marks}"{value}"{marks}'
    return json.dumps(value)


def load_excluded_name_keys() -> set[str]:
    raw_path = os.environ.get("IRONSMITH_GENERATED_REGISTRY_SKIP_NAMES_FILE", "").strip()
    if not raw_path:
        return set()

    path = Path(raw_path)
    if not path.exists():
        print(f"[generate_baked_registry] skip-list file not found: {path}")
        return set()

    excluded: set[str] = set()
    for line in path.read_text(encoding="utf-8").splitlines():
        name = line.strip()
        if name:
            excluded.add(name.casefold())
    return excluded


FlipPair = Tuple[str, str, str, str, str]


def collect_unique_blocks(
    excluded_name_keys: set[str],
) -> Tuple[Dict[str, Tuple[str, str]], List[FlipPair]]:
    unique: Dict[str, Tuple[str, str]] = {}
    flips: List[FlipPair] = []
    for card in iter_json_array(CARDS_JSON):
        oracle_text = card_oracle_text(card)
        if oracle_text and has_digital_only_oracle_marker(oracle_text):
            continue

        layout = (card.get("layout") or "").strip().lower()
        faces = card.get("card_faces") or []

        # Flip cards need both faces available at runtime for the Flip effect.
        # We generate both faces and wire `Card.other_face` on the front face.
        if layout == "flip" and isinstance(faces, list) and len(faces) >= 2:
            front = faces[0]
            back = faces[1]
            combined_name = (card.get("name") or "").strip()
            if not isinstance(front, dict) or not isinstance(back, dict) or not combined_name:
                continue

            def parse_block_for_face(face: dict) -> Tuple[str, str] | None:
                name = (face.get("name") or "").strip()
                mana_cost = face.get("mana_cost")
                type_line = face.get("type_line")
                oracle_text = face.get("oracle_text")
                power = face.get("power")
                toughness = face.get("toughness")
                loyalty = face.get("loyalty")
                defense = face.get("defense")

                if not name or not type_line or not oracle_text:
                    return None
                if is_non_playable(card, type_line, oracle_text):
                    return None

                lines = []
                if mana_cost:
                    lines.append(f"Mana cost: {mana_cost}")
                lines.append(f"Type: {type_line}")
                if power is not None and toughness is not None:
                    lines.append(f"Power/Toughness: {power}/{toughness}")
                if loyalty is not None:
                    lines.append(f"Loyalty: {loyalty}")
                if defense is not None:
                    lines.append(f"Defense: {defense}")
                lines.append(oracle_text)
                return (name, "\n".join(lines).strip())

            front_pair = parse_block_for_face(front)
            back_pair = parse_block_for_face(back)
            if not front_pair or not back_pair:
                continue

            front_name, front_parse_block = front_pair
            back_name, back_parse_block = back_pair

            if (
                front_name.casefold() in excluded_name_keys
                or back_name.casefold() in excluded_name_keys
                or combined_name.casefold() in excluded_name_keys
            ):
                continue

            flips.append(
                (front_name, front_parse_block, back_name, back_parse_block, combined_name)
            )
            continue

        block = build_block(card)
        if not block:
            continue
        lines = block.splitlines()
        first_line = lines[0] if lines else ""
        if not first_line.startswith("Name: "):
            continue
        name = first_line.removeprefix("Name: ").strip()
        if not name:
            continue
        # Keep metadata + oracle text only. Name is provided by the Rust builder.
        # Leaving "Name:" inside the parse input causes strict parser failures.
        parse_block = "\n".join(lines[1:]).strip()
        key = name.casefold()
        if key in excluded_name_keys:
            continue
        if key not in unique:
            unique[key] = (name, parse_block)
    return unique, flips


def write_generated_source(
    cards: Dict[str, Tuple[str, str]], flips: List[FlipPair], output_path: Path
) -> None:
    ordered = sorted(cards.values(), key=lambda pair: pair[0].casefold())
    flips_ordered = sorted(flips, key=lambda pair: pair[0].casefold())

    lines = []
    lines.append("// @generated by scripts/generate_baked_registry.py")
    lines.append("// Do not edit manually.")
    lines.append("")
    lines.append("use super::{CardDefinition, CardDefinitionBuilder, CardRegistry};")
    lines.append("use crate::ids::CardId;")
    lines.append("use std::sync::OnceLock;")
    lines.append("")
    lines.append(
        f"pub const GENERATED_PARSER_CARD_SOURCE_COUNT: usize = {len(ordered) + 2 * len(flips_ordered)};"
    )
    lines.append("")
    lines.append("const GENERATED_ALIASES: &[(&str, &str)] = &[")
    for front_name, _front_block, _back_name, _back_block, combined_name in flips_ordered:
        lines.append(
            f"    ({rust_raw_literal(combined_name)}, {rust_raw_literal(front_name)}),"
        )
    lines.append("];")
    lines.append("")
    lines.append("fn parse_generated_card(cards: &mut Vec<CardDefinition>, name: &str, block: &str) {")
    lines.append("    let builder = CardDefinitionBuilder::new(CardId::new(), name);")
    lines.append("    if let Ok(definition) = builder.parse_text(block.to_string()) {")
    lines.append("        if super::generated_definition_is_supported(&definition) {")
    lines.append("            cards.push(definition);")
    lines.append("        }")
    lines.append("    }")
    lines.append("}")
    lines.append("")
    lines.append("fn parse_generated_flip_card(")
    lines.append("    cards: &mut Vec<CardDefinition>,")
    lines.append("    front_name: &str,")
    lines.append("    front_block: &str,")
    lines.append("    back_name: &str,")
    lines.append("    back_block: &str,")
    lines.append(") {")
    lines.append("    let front_id = CardId::new();")
    lines.append("    let back_id = CardId::new();")
    lines.append("    let front_builder = CardDefinitionBuilder::new(front_id, front_name);")
    lines.append("    let back_builder = CardDefinitionBuilder::new(back_id, back_name);")
    lines.append("    let Ok(mut front) = front_builder.parse_text(front_block.to_string()) else { return; };")
    lines.append("    let Ok(back) = back_builder.parse_text(back_block.to_string()) else { return; };")
    lines.append("    if !super::generated_definition_is_supported(&front) { return; }")
    lines.append("    if !super::generated_definition_is_supported(&back) { return; }")
    lines.append("    front.card.other_face = Some(back_id);")
    lines.append("    cards.push(front);")
    lines.append("    cards.push(back);")
    lines.append("}")
    lines.append("")
    lines.append("fn parsed_generated_cards() -> &'static Vec<CardDefinition> {")
    lines.append("    static PARSED: OnceLock<Vec<CardDefinition>> = OnceLock::new();")
    lines.append("    PARSED.get_or_init(|| {")
    lines.append("        let mut cards = Vec::new();")
    for name, block in ordered:
        name_lit = rust_raw_literal(name)
        block_lit = rust_raw_literal(block)
        lines.append(f"        parse_generated_card(&mut cards, {name_lit}, {block_lit});")
    for front_name, front_block, back_name, back_block, _combined_name in flips_ordered:
        lines.append("        parse_generated_flip_card(")
        lines.append("            &mut cards,")
        lines.append(f"            {rust_raw_literal(front_name)},")
        lines.append(f"            {rust_raw_literal(front_block)},")
        lines.append(f"            {rust_raw_literal(back_name)},")
        lines.append(f"            {rust_raw_literal(back_block)},")
        lines.append("        );")
    lines.append("        cards")
    lines.append("    })")
    lines.append("}")
    lines.append("")
    lines.append("pub fn register_generated_parser_cards(registry: &mut CardRegistry) {")
    lines.append("    for definition in parsed_generated_cards() {")
    lines.append("        if registry.get(definition.card.name.as_str()).is_none() {")
    lines.append("            registry.register(definition.clone());")
    lines.append("        }")
    lines.append("    }")
    lines.append("    for (alias, canonical) in GENERATED_ALIASES {")
    lines.append("        registry.register_alias(*alias, *canonical);")
    lines.append("    }")
    lines.append("}")
    lines.append("")

    output_path.parent.mkdir(parents=True, exist_ok=True)
    output_path.write_text("\n".join(lines), encoding="utf-8")


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Generate baked parser registry source from cards.json"
    )
    parser.add_argument(
        "--out",
        dest="out",
        default=str(OUT_FILE),
        help="Output Rust source file path",
    )
    return parser.parse_args()


def main() -> None:
    args = parse_args()
    output_path = Path(args.out)
    excluded_name_keys = load_excluded_name_keys()
    cards, flips = collect_unique_blocks(excluded_name_keys)
    write_generated_source(cards, flips, output_path)
    print(
        f"wrote {output_path} with {len(cards) + 2 * len(flips)} source cards "
        f"(excluded by threshold: {len(excluded_name_keys)})"
    )


if __name__ == "__main__":
    main()
