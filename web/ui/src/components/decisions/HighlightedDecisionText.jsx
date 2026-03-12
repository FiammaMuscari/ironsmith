import { useMemo } from "react";
import { SymbolText } from "@/lib/mana-symbols";

function splitHighlightedText(text, highlightText) {
  const source = String(text || "");
  const needle = String(highlightText || "").trim();
  if (!source || !needle) {
    return { before: source, match: "", after: "" };
  }

  const sourceLower = source.toLowerCase();
  const needleLower = needle.toLowerCase();
  const matchIndex = sourceLower.indexOf(needleLower);
  if (matchIndex < 0) {
    return { before: source, match: "", after: "" };
  }

  return {
    before: source.slice(0, matchIndex),
    match: source.slice(matchIndex, matchIndex + needle.length),
    after: source.slice(matchIndex + needle.length),
  };
}

export default function HighlightedDecisionText({
  text,
  highlightText = "",
  highlightColor = null,
  className = "",
  style = undefined,
}) {
  const normalizedText = String(text || "");
  const segments = useMemo(
    () => splitHighlightedText(normalizedText, highlightText),
    [highlightText, normalizedText]
  );

  if (!highlightColor || !segments.match) {
    return (
      <span className={className} style={style}>
        <SymbolText text={normalizedText} style={{ whiteSpace: "inherit" }} />
      </span>
    );
  }

  return (
    <span className={className} style={style}>
      {segments.before && (
        <SymbolText text={segments.before} style={{ whiteSpace: "inherit" }} />
      )}
      <span style={{ color: highlightColor }}>
        <SymbolText text={segments.match} style={{ whiteSpace: "inherit" }} />
      </span>
      {segments.after && (
        <SymbolText text={segments.after} style={{ whiteSpace: "inherit" }} />
      )}
    </span>
  );
}
