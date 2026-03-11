import { useMemo, useState } from "react";
import { useGame } from "@/context/GameContext";
import { Badge } from "@/components/ui/badge";
import { Slider } from "@/components/ui/slider";
import {
  findSavedDeckPreset,
  listSavedDeckPresets,
  parseDeckList,
  saveSavedDeckPreset,
} from "@/lib/decklists";

const pill = "text-[13px] uppercase cursor-pointer hover:brightness-125 transition-all select-none";
const fieldClass =
  "w-full rounded-md border border-[#344a61] bg-[#0b1118] px-3 py-2 text-[13px] text-foreground outline-none focus:border-primary/60";

function samePresetTexts(left, right) {
  const leftTexts = Array.isArray(left) ? left : [];
  const rightTexts = Array.isArray(right) ? right : [];
  if (leftTexts.length !== rightTexts.length) return false;
  return leftTexts.every((text, index) => String(text || "") === String(rightTexts[index] || ""));
}

function fitTextsToPlayers(players, texts) {
  return players.map((_, index) => String(texts?.[index] || ""));
}

export default function DeckLoadingView({ onLoad, onCancel }) {
  const {
    state,
    setStatus,
    semanticThreshold,
    setSemanticThreshold,
    cardsMeetingThreshold,
  } = useGame();
  const players = state?.players || [];
  const [texts, setTexts] = useState(() => players.map(() => ""));
  const [savedPresets, setSavedPresets] = useState(() => listSavedDeckPresets());
  const [selectedPresetName, setSelectedPresetName] = useState("");
  const [presetName, setPresetName] = useState("");

  const handleTextChange = (index, value) => {
    setTexts((prev) => {
      const next = [...prev];
      next[index] = value;
      return next;
    });
  };

  const cardCounts = useMemo(
    () => texts.map((t) => parseDeckList(t).length),
    [texts]
  );
  const totalCards = cardCounts.reduce((a, b) => a + b, 0);

  const selectedPreset = useMemo(
    () =>
      savedPresets.find(
        (preset) => preset.name === selectedPresetName
      ) || null,
    [savedPresets, selectedPresetName]
  );

  const handleApplySavedPreset = () => {
    if (!selectedPreset) return;
    setTexts(fitTextsToPlayers(players, selectedPreset.texts));
    setPresetName(selectedPreset.name);
  };

  const handleLoad = () => {
    const decks = texts.map(parseDeckList);
    const normalizedPresetName = presetName.trim();

    if (normalizedPresetName) {
      const existingPreset = findSavedDeckPreset(normalizedPresetName);
      const nextTexts = fitTextsToPlayers(players, texts);
      const shouldConfirmOverride =
        existingPreset && !samePresetTexts(existingPreset.texts, nextTexts);
      if (
        shouldConfirmOverride
        && !window.confirm(`A saved deck named "${existingPreset.name}" already exists. Override it?`)
      ) {
        onLoad(decks);
        return;
      }

      const saveResult = saveSavedDeckPreset(normalizedPresetName, nextTexts);
      if (saveResult.saved) {
        setSavedPresets(saveResult.entries);
        setSelectedPresetName(saveResult.entry.name);
        setPresetName(saveResult.entry.name);
        setStatus(
          saveResult.replaced
            ? `Updated saved deck "${saveResult.entry.name}"`
            : `Saved deck "${saveResult.entry.name}"`
        );
      }
    }

    onLoad(decks);
  };

  return (
    <main
      className="table-gradient rounded grid gap-1.5 p-1.5 min-h-0 overflow-hidden"
      style={{ gridTemplateRows: "auto minmax(0,1fr) auto" }}
    >
      <div className="grid gap-3 rounded border border-[#2b3e55] bg-[#09111a] p-3 lg:grid-cols-[minmax(0,1fr)_300px]">
        <label className="grid gap-1 text-[11px] uppercase tracking-[0.18em] text-[#8fb1d6]">
          Saved Deck
          <div className="flex gap-2">
            <select
              className={fieldClass}
              value={selectedPresetName}
              onChange={(event) => setSelectedPresetName(event.target.value)}
            >
              <option value="">Select a saved deck</option>
              {savedPresets.map((preset) => (
                <option key={preset.name} value={preset.name}>
                  {preset.name}
                </option>
              ))}
            </select>
            <Badge
              variant="secondary"
              className={`${pill} px-3 ${selectedPreset ? "" : "pointer-events-none opacity-45"}`}
              onClick={selectedPreset ? handleApplySavedPreset : undefined}
            >
              Use Saved
            </Badge>
          </div>
        </label>
        <label className="grid gap-1 text-[11px] uppercase tracking-[0.18em] text-[#8fb1d6]">
          Save As
          <input
            className={fieldClass}
            placeholder="Friday gauntlet"
            value={presetName}
            onChange={(event) => setPresetName(event.target.value)}
          />
        </label>
      </div>
      <div
        className="grid gap-1.5 min-h-0"
        style={{ gridTemplateColumns: `repeat(${players.length}, minmax(0, 1fr))` }}
      >
        {players.map((player, i) => (
          <div
            key={player.id}
            className="border border-[#2b3e55] bg-gradient-to-b from-[#101826] to-[#0a121d] p-2 grid gap-1.5 min-h-0"
            style={{ gridTemplateRows: "auto minmax(0,1fr)" }}
          >
            <div className="flex justify-between items-baseline">
              <span className="text-[16px] text-[#a4bdd7] uppercase tracking-wider font-bold">
                {player.name}
              </span>
              <span className="text-[14px] text-muted-foreground">
                {cardCounts[i]} cards
              </span>
            </div>
            <textarea
              className="w-full min-h-0 h-full bg-[#0b1118] border border-[#344a61] text-foreground text-[14px] p-2 rounded resize-none font-mono focus:outline-none focus:border-primary/50"
              placeholder={`Paste ${player.name}'s deck list...\n\nMTGA / MTGO / Moxfield format:\n4 Lightning Bolt\n2 Counterspell\n20 Island`}
              value={texts[i] || ""}
              onChange={(e) => handleTextChange(i, e.target.value)}
            />
          </div>
        ))}
      </div>
      <div className="flex items-center justify-between gap-3 py-1">
        <div className="flex min-w-0 flex-1 items-center gap-2">
          <span className="text-[12px] font-semibold uppercase tracking-wide text-[#8fb1d6] whitespace-nowrap">
            Min similarity
          </span>
          <Slider
            className="w-28"
            min={0}
            max={100}
            step={1}
            value={[Math.round(semanticThreshold)]}
            onValueChange={([value]) => setSemanticThreshold(value)}
          />
          <span className="text-[12px] text-muted-foreground whitespace-nowrap">
            {semanticThreshold > 0 ? `${Math.round(semanticThreshold)}%` : "Off"} ({cardsMeetingThreshold})
          </span>
        </div>
        <div className="flex items-center justify-center gap-2">
        <Badge variant="secondary" className={`${pill} px-4`} onClick={handleLoad}>
          Load{totalCards > 0 ? ` (${totalCards} cards)` : ""}
        </Badge>
        <Badge variant="secondary" className={pill} onClick={onCancel}>
          Cancel
        </Badge>
        </div>
      </div>
    </main>
  );
}
