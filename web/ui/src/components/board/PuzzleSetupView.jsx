import { useEffect, useMemo, useState } from "react";

import { useGame } from "@/context/GameContext";
import { Button } from "@/components/ui/button";
import { Badge } from "@/components/ui/badge";
import { copyTextToClipboard } from "@/lib/clipboard";
import {
  PUZZLE_ZONE_ORDER,
  buildPuzzlePayload,
  buildPuzzlePayloadFromGameState,
  buildPuzzleUrl,
  buildPuzzleZoneTextsFromPayload,
  clearSavedPuzzleDraft,
  createEmptyPuzzleZoneTexts,
  createPuzzlePlayers,
  fitPuzzleZoneTextsToPlayers,
  loadSavedPuzzleDraft,
  parsePuzzleCardList,
  saveSavedPuzzleDraft,
} from "@/lib/puzzles";

const fieldClass =
  "w-full rounded-md border border-[#344a61] bg-[#0b1118] px-3 py-2 text-[13px] text-foreground outline-none focus:border-primary/60";
const zoneLabelClass =
  "grid gap-1 text-[10px] uppercase tracking-[0.18em] text-[#8fb1d6]";
const MAX_PLAYERS = 8;

function zoneTitle(zone) {
  switch (zone) {
    case "battlefield": return "Battlefield";
    case "hand": return "Hand";
    case "graveyard": return "Graveyard";
    case "exile": return "Exile";
    case "library": return "Library";
    case "command": return "Command";
    default: return zone;
  }
}

function puzzleDraftFromGameState(state) {
  const payload = buildPuzzlePayloadFromGameState(state);
  if (!payload) {
    const players = createPuzzlePlayers(2);
    return {
      players,
      zoneTexts: createEmptyPuzzleZoneTexts(players),
    };
  }

  const players = payload.players.map((player, index) => ({
    id: `puzzle-player-${index + 1}`,
    name: player.name,
    life: player.life,
  }));

  return {
    players,
    zoneTexts: fitPuzzleZoneTextsToPlayers(players, buildPuzzleZoneTextsFromPayload(payload)),
  };
}

export default function PuzzleSetupView({ onLoadPuzzle, onCancel }) {
  const { state, setStatus } = useGame();
  const initialDraft = useMemo(
    () => {
      const savedDraft = loadSavedPuzzleDraft();
      if (savedDraft) return { ...savedDraft, restored: true };
      return { ...puzzleDraftFromGameState(state), restored: false };
    },
    [state]
  );
  const [players, setPlayers] = useState(initialDraft.players);
  const [zoneTexts, setZoneTexts] = useState(initialDraft.zoneTexts);

  useEffect(() => {
    if (!initialDraft.restored) return;
    setStatus("Restored saved puzzle draft");
  }, [initialDraft.restored, setStatus]);

  useEffect(() => {
    saveSavedPuzzleDraft(players, zoneTexts);
  }, [players, zoneTexts]);

  const payload = useMemo(() => buildPuzzlePayload(players, zoneTexts), [players, zoneTexts]);
  const shareUrl = useMemo(() => buildPuzzleUrl(payload), [payload]);
  const totalCards = useMemo(
    () => payload.players.reduce(
      (count, player) => count + PUZZLE_ZONE_ORDER.reduce(
        (zoneCount, zone) => zoneCount + (player.zones?.[zone]?.length || 0),
        0
      ),
      0
    ),
    [payload]
  );

  const updateZoneText = (playerIndex, zone, value) => {
    setZoneTexts((current) => current.map((entry, index) => (
      index === playerIndex ? { ...entry, [zone]: value } : entry
    )));
  };

  const updatePlayerName = (playerIndex, value) => {
    setPlayers((current) => current.map((player, index) => (
      index === playerIndex ? { ...player, name: value } : player
    )));
  };

  const updatePlayerLife = (playerIndex, value) => {
    setPlayers((current) => current.map((player, index) => (
      index === playerIndex
        ? { ...player, life: Number(value) || 0 }
        : player
    )));
  };

  const adjustPlayerCount = (nextCount) => {
    const boundedCount = Math.max(1, Math.min(MAX_PLAYERS, Number(nextCount) || 1));
    setPlayers((current) => {
      if (current.length === boundedCount) return current;
      const nextPlayers = current.slice(0, boundedCount);
      for (let index = nextPlayers.length; index < boundedCount; index += 1) {
        nextPlayers.push({ id: `puzzle-player-${index + 1}`, name: `Player ${index + 1}`, life: 20 });
      }
      return nextPlayers;
    });
    setZoneTexts((current) => fitPuzzleZoneTextsToPlayers(createPuzzlePlayers(boundedCount), current));
  };

  const handleImportCurrentTable = () => {
    const imported = puzzleDraftFromGameState(state);
    setPlayers(imported.players);
    setZoneTexts(imported.zoneTexts);
    setStatus("Imported visible cards from the current table");
  };

  const handleClearDraft = () => {
    const cleared = puzzleDraftFromGameState(state);
    clearSavedPuzzleDraft();
    setPlayers(cleared.players);
    setZoneTexts(cleared.zoneTexts);
    setStatus("Cleared saved puzzle draft");
  };

  const handleCopyLink = async () => {
    if (!shareUrl) {
      setStatus("Could not generate a puzzle link", true);
      return;
    }

    const copied = await copyTextToClipboard(shareUrl);
    setStatus(copied ? "Copied puzzle link" : "Could not copy puzzle link", !copied);
  };

  const handleLoadHere = async () => {
    if (typeof onLoadPuzzle !== "function") return;
    await onLoadPuzzle(payload, "Puzzle loaded");
  };

  return (
    <main
      className="table-gradient rounded grid h-full gap-3 overflow-y-auto overflow-x-hidden p-3"
      style={{ gridTemplateRows: "auto auto auto" }}
    >
      <div className="grid gap-3 rounded border border-[#2b3e55] bg-[#09111a] p-4 lg:grid-cols-[minmax(0,1fr)_380px]">
        <div className="grid gap-3">
          <div className="grid gap-2">
            <div className="text-[11px] uppercase tracking-[0.24em] text-[#cdb27a]">
              Puzzle Setup
            </div>
            <div className="text-[22px] font-bold uppercase tracking-[0.16em] text-foreground">
              Share A Board Position
            </div>
            <p className="m-0 max-w-[64ch] text-[13px] leading-5 text-muted-foreground">
              Fill any zone for each player, then copy the generated `?puzzle=` link. Loading that
              link resets the table and places the listed cards directly
              into those zones without triggering ETBs.
            </p>
            <p className="m-0 max-w-[64ch] text-[12px] leading-5 text-[#8fb1d6]">
              Importing from the current table includes visible zones only. Libraries and hidden
              opponent hands stay blank unless you type them in here.
            </p>
          </div>

          <div className="grid gap-3 rounded border border-[#2b3e55] bg-[#0b1118] p-3">
            <div className="flex flex-wrap items-center gap-2">
              <Badge variant="secondary" className="stone-pill px-3 uppercase">
                {players.length} player{players.length === 1 ? "" : "s"}
              </Badge>
              <Badge variant="secondary" className="stone-pill px-3 uppercase">
                {totalCards} card{totalCards === 1 ? "" : "s"}
              </Badge>
            </div>
            <div className="flex flex-wrap items-center gap-2">
              <Button
                type="button"
                variant="secondary"
                className="stone-pill"
                disabled={players.length <= 1}
                onClick={() => adjustPlayerCount(players.length - 1)}
              >
                Remove Player
              </Button>
              <Button
                type="button"
                variant="secondary"
                className="stone-pill"
                disabled={players.length >= MAX_PLAYERS}
                onClick={() => adjustPlayerCount(players.length + 1)}
              >
                Add Player
              </Button>
              <Button type="button" variant="secondary" className="stone-pill" onClick={handleImportCurrentTable}>
                Import Current Table
              </Button>
              <Button type="button" variant="secondary" className="stone-pill" onClick={handleClearDraft}>
                Clear Draft
              </Button>
            </div>
          </div>
        </div>

        <div className="grid gap-3 rounded border border-[#2b3e55] bg-[#0b1118] p-3">
          <label className={zoneLabelClass}>
            Share Link
            <textarea
              className="min-h-[120px] w-full rounded-md border border-[#344a61] bg-[#081018] px-3 py-2 font-mono text-[12px] text-foreground outline-none"
              readOnly
              value={shareUrl}
            />
          </label>
          <div className="grid gap-2 sm:grid-cols-2">
            <Button type="button" variant="secondary" className="stone-pill" onClick={handleCopyLink}>
              Copy Link
            </Button>
            <Button type="button" variant="secondary" className="stone-pill" onClick={handleLoadHere}>
              Load Here
            </Button>
          </div>
        </div>
      </div>

      <div
        className="grid gap-3 overflow-x-auto overflow-y-visible"
        style={{ gridTemplateColumns: `repeat(${Math.max(players.length, 1)}, minmax(280px, 1fr))` }}
      >
        {players.map((player, playerIndex) => {
          const playerPayload = payload.players[playerIndex];
          return (
            <section
              key={player.id}
              className="grid gap-3 min-h-0 rounded border border-[#2b3e55] bg-gradient-to-b from-[#101826] to-[#0a121d] p-3"
            >
              <div className="grid gap-2">
                <label className={zoneLabelClass}>
                  Player Name
                  <input
                    className={fieldClass}
                    value={player.name}
                    onChange={(event) => updatePlayerName(playerIndex, event.target.value)}
                    placeholder={`Player ${playerIndex + 1}`}
                  />
                </label>
                <label className={zoneLabelClass}>
                  Life
                  <input
                    className={fieldClass}
                    type="number"
                    value={player.life}
                    onChange={(event) => updatePlayerLife(playerIndex, event.target.value)}
                  />
                </label>
                <div className="text-[12px] text-muted-foreground">
                  Life {playerPayload?.life ?? 20} - {" "}
                  {PUZZLE_ZONE_ORDER.reduce(
                    (count, zone) => count + (playerPayload?.zones?.[zone]?.length || 0),
                    0
                  )} cards encoded
                </div>
              </div>

              <div className="grid gap-3">
                {PUZZLE_ZONE_ORDER.map((zone) => (
                  <label key={`${player.id}:${zone}`} className={zoneLabelClass}>
                    <span className="flex items-center justify-between gap-2">
                      <span>{zoneTitle(zone)}</span>
                      <span className="text-[11px] text-muted-foreground">
                        {parsePuzzleCardList(zoneTexts[playerIndex]?.[zone]).length}
                      </span>
                    </span>
                    <textarea
                      className={`${fieldClass} min-h-[92px] resize-y font-mono`}
                      placeholder={`1 ${player.name || `Player ${playerIndex + 1}`} card per line`}
                      value={zoneTexts[playerIndex]?.[zone] || ""}
                      onChange={(event) => updateZoneText(playerIndex, zone, event.target.value)}
                    />
                  </label>
                ))}
              </div>
            </section>
          );
        })}
      </div>

      <div className="flex items-center justify-end gap-2">
        <Button type="button" variant="secondary" className="stone-pill" onClick={onCancel}>
          Back To Table
        </Button>
      </div>
    </main>
  );
}
