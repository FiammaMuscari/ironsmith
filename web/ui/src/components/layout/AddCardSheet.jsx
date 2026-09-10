import { useState, useCallback, useEffect, useRef, useId } from "react";
import { useGame } from "@/context/GameContext";
import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
import { useI18n } from "@/i18n/I18nContext";
import { resolveCardNameForGame } from "@/lib/card-name-resolution";
import {
  Sheet,
  SheetContent,
  SheetDescription,
  SheetHeader,
  SheetTitle,
  SheetTrigger,
} from "@/components/ui/sheet";

const inputClass =
  "fantasy-field w-full px-3 py-2 text-[14px] text-foreground outline-none disabled:cursor-not-allowed disabled:opacity-50";
const labelClass =
  "grid gap-1 text-[11px] uppercase tracking-[0.2em] text-muted-foreground";
const selectClass =
  "fantasy-field w-full px-3 py-2 text-[14px] text-foreground outline-none disabled:cursor-not-allowed disabled:opacity-50";

function formatAddCardFailureClipboard(cardName, zone, errorMessage) {
  return [
    cardName ? `Card: ${cardName}` : "",
    zone ? `Zone: ${zone}` : "",
    errorMessage ? `Error: ${errorMessage}` : "",
  ]
    .filter(Boolean)
    .join("\n\n");
}

export default function AddCardSheet({
  trigger,
  onAddCardNotice,
  triggerClassName = "",
}) {
  const {
    game,
    state,
    refresh,
    runWasmInteraction,
    setStatus,
    multiplayer,
  } = useGame();
  const { locale, t } = useI18n();
  const [open, setOpen] = useState(false);
  const [cardName, setCardName] = useState("");
  const [zone, setZone] = useState("hand");
  const [playerIndex, setPlayerIndex] = useState(null);
  const [skipTriggers, setSkipTriggers] = useState(false);
  const [autocompleteOptions, setAutocompleteOptions] = useState([]);
  const [autocompleteOpen, setAutocompleteOpen] = useState(false);
  const [autocompleteIndex, setAutocompleteIndex] = useState(-1);
  const autocompleteId = useId();
  const autocompleteRef = useRef(null);
  const cardNameInputRef = useRef(null);
  const suppressAutocompleteRef = useRef(false);
  const autocompleteRequestRef = useRef(0);

  const players = state?.players || [];
  const perspective = state?.perspective ?? 0;
  const selectedPlayer = playerIndex ?? perspective;
  const addLocked = multiplayer.mode !== "idle" && !multiplayer.matchStarted;

  const visibleAutocompleteOptions =
    addLocked || !cardName.trim() ? [] : autocompleteOptions;
  const autocompleteVisible =
    autocompleteOpen && visibleAutocompleteOptions.length > 0;

  useEffect(() => {
    const query = cardName.trim();
    if (addLocked || !query || !game || typeof game.autocompleteCardNames !== "function") return;

    if (suppressAutocompleteRef.current) {
      suppressAutocompleteRef.current = false;
      return;
    }

    const requestId = autocompleteRequestRef.current + 1;
    autocompleteRequestRef.current = requestId;
    const timeoutId = window.setTimeout(async () => {
      try {
        let matches = await game.autocompleteCardNames(query, 5);
        // The embedded registry is fast and authoritative. Only when it has
        // no match do we ask Scryfall to resolve a localized printed name.
        if (matches.length === 0 && query.length >= 3) {
          const resolved = await resolveCardNameForGame({ game, cardName: query, locale });
          if (resolved.status === "available") matches = [resolved.canonicalName];
        }
        if (autocompleteRequestRef.current !== requestId) return;
        setAutocompleteOptions(matches);
        setAutocompleteOpen(matches.length > 0);
        setAutocompleteIndex(matches.length === 1 ? 0 : -1);
      } catch (error) {
        if (autocompleteRequestRef.current !== requestId) return;
        console.warn("Autocomplete lookup failed:", error);
        setAutocompleteOptions([]);
        setAutocompleteOpen(false);
        setAutocompleteIndex(-1);
      }
    }, 150);

    return () => {
      window.clearTimeout(timeoutId);
    };
  }, [addLocked, cardName, game, locale]);

  useEffect(() => {
    const handlePointerDown = (event) => {
      if (!autocompleteRef.current?.contains(event.target)) {
        setAutocompleteOpen(false);
        setAutocompleteIndex(-1);
      }
    };

    window.addEventListener("pointerdown", handlePointerDown);
    return () => window.removeEventListener("pointerdown", handlePointerDown);
  }, []);

  useEffect(() => {
    if (!open) return;

    const frameId = window.requestAnimationFrame(() => {
      cardNameInputRef.current?.focus();
    });

    return () => window.cancelAnimationFrame(frameId);
  }, [open]);

  const closeSheet = useCallback(() => {
    setOpen(false);
    setAutocompleteOpen(false);
    setAutocompleteIndex(-1);
  }, []);

  const handleAdd = useCallback(async (requestedName = cardName) => {
    return runWasmInteraction(async () => {
      if (addLocked) {
        setStatus("Card injection is disabled while a lobby is active", true);
        return;
      }
      const requestedCardName = String(requestedName || "").trim();
      if (!requestedCardName) {
        setStatus("Enter a card name to add", true);
        return;
      }
      if (!game || typeof game.addCardToZone !== "function") {
        setStatus("This WASM build does not expose addCardToZone", true);
        return;
      }
      try {
        const resolution = await resolveCardNameForGame({
          game,
          cardName: requestedCardName,
          locale,
        });
        if (resolution.status === "not-embedded") {
          const message = `${resolution.canonicalName} exists, but is not embedded in this game build`;
          setStatus(message, true);
          if (typeof onAddCardNotice === "function") {
            onAddCardNotice({
              tone: "error",
              title: `Card is unavailable: ${resolution.canonicalName}`,
              body: "The name was resolved safely, but this engine build does not contain that card.",
              copyText: formatAddCardFailureClipboard(resolution.canonicalName, zone, message),
              copyStatusMessage: `Copied diagnostics for ${resolution.canonicalName}`,
            });
          }
          return;
        }
        if (resolution.status !== "available") {
          setStatus(`No exact card match found for ${requestedCardName}`, true);
          return;
        }
        const name = resolution.canonicalName;
        await game.addCardToZone(selectedPlayer, name, zone, skipTriggers);
        const injectedDuringMatch = Boolean(multiplayer.matchStarted);
        setCardName("");
        setAutocompleteOptions([]);
        setAutocompleteOpen(false);
        setAutocompleteIndex(-1);
        closeSheet();
        // Keep the add-card critical path limited to the mutation + refresh.
        // Follow-up diagnostics are intentionally omitted here because a hung
        // worker call would leave the shared interaction gate blocked.
        await refresh(
          injectedDuringMatch
            ? `Added ${name} locally; peers will reject it if used`
            : `Added ${name} to ${zone}`
        );
      } catch (err) {
        const errMsg = String(err?.message || err);
        const name = requestedCardName;
        setStatus(`Add card failed: ${errMsg}`, true);
        if (typeof onAddCardNotice === "function") {
          onAddCardNotice({
            tone: "error",
            title: `Could not add ${name}`,
            body: `${errMsg} Click to copy diagnostics.`,
            copyText: formatAddCardFailureClipboard(name, zone, errMsg),
            copyStatusMessage: `Copied diagnostics for ${name}`,
          });
        }
      }
    });
  }, [
    addLocked,
    cardName,
    closeSheet,
    game,
    locale,
    onAddCardNotice,
    refresh,
    runWasmInteraction,
    selectedPlayer,
    setStatus,
    skipTriggers,
    zone,
    multiplayer.matchStarted,
  ]);

  const handleAutocompletePick = useCallback((name) => {
    suppressAutocompleteRef.current = true;
    setCardName(name);
    setAutocompleteOptions([]);
    setAutocompleteOpen(false);
    setAutocompleteIndex(-1);
    window.requestAnimationFrame(() => {
      cardNameInputRef.current?.focus();
    });
  }, []);

  return (
    <Sheet open={open} onOpenChange={setOpen}>
      <SheetTrigger asChild>
        {trigger}
      </SheetTrigger>
      <SheetContent
        side="center"
        className={`fantasy-sheet add-card-sheet w-[min(92vw,460px)] p-0 ${triggerClassName}`}
      >
        <SheetHeader className="fantasy-sheet-header pr-12">
          <div className="text-[11px] uppercase tracking-[0.24em] text-[#cdb27a]">{t("addCard.eyebrow")}</div>
          <SheetTitle className="text-[22px] uppercase tracking-[0.18em] text-foreground">
            {t("addCard.title")}
          </SheetTitle>
          <SheetDescription className="max-w-[34ch] text-[13px] leading-5">
            {t("addCard.description")}
          </SheetDescription>
        </SheetHeader>

        <div className="add-card-sheet-body grid gap-4 p-4">
          <div className="relative grid gap-1" ref={autocompleteRef}>
            <label className={labelClass}>
              {t("addCard.cardName")}
              <input
                ref={cardNameInputRef}
                role="combobox"
                aria-autocomplete="list"
                aria-expanded={autocompleteVisible}
                aria-controls={autocompleteVisible ? autocompleteId : undefined}
                aria-activedescendant={autocompleteVisible && autocompleteIndex >= 0 ? `${autocompleteId}-${autocompleteIndex}` : undefined}
                className={inputClass}
                placeholder={t("addCard.cardName")}
                value={cardName}
                disabled={addLocked}
                onChange={(event) => {
                  setCardName(event.target.value);
                  setAutocompleteOpen(true);
                  setAutocompleteIndex(-1);
                }}
                onFocus={() => {
                  if (visibleAutocompleteOptions.length > 0) {
                    setAutocompleteOpen(true);
                  }
                }}
                onKeyDown={(event) => {
                  if (event.key === "ArrowDown" && visibleAutocompleteOptions.length > 0) {
                    event.preventDefault();
                    setAutocompleteOpen(true);
                    setAutocompleteIndex((prev) => (
                      prev >= visibleAutocompleteOptions.length - 1 ? 0 : prev + 1
                    ));
                    return;
                  }

                  if (event.key === "ArrowUp" && visibleAutocompleteOptions.length > 0) {
                    event.preventDefault();
                    setAutocompleteOpen(true);
                    setAutocompleteIndex((prev) => (
                      prev <= 0 ? visibleAutocompleteOptions.length - 1 : prev - 1
                    ));
                    return;
                  }

                  if (event.key === "Escape" && autocompleteVisible) {
                    event.preventDefault();
                    event.stopPropagation();
                    setAutocompleteOpen(false);
                    setAutocompleteIndex(-1);
                    return;
                  }

                  if (event.key === "Enter") {
                    event.preventDefault();
                    if (
                      autocompleteVisible
                      && autocompleteIndex >= 0
                      && visibleAutocompleteOptions[autocompleteIndex]
                    ) {
                      handleAdd(visibleAutocompleteOptions[autocompleteIndex]);
                      return;
                    }
                    if (visibleAutocompleteOptions.length === 1) {
                      handleAdd(visibleAutocompleteOptions[0]);
                      return;
                    }
                    handleAdd();
                  }
                }}
              />
            </label>
            {autocompleteVisible ? (
              <div id={autocompleteId} role="listbox" aria-label="Matching cards" className="add-card-autocomplete absolute left-0 top-[calc(100%+0.35rem)] z-40 w-full overflow-hidden p-1">
                {visibleAutocompleteOptions.map((option, index) => (
                  <button
                    key={option}
                    id={`${autocompleteId}-${index}`}
                    role="option"
                    aria-selected={index === autocompleteIndex}
                    tabIndex={-1}
                    type="button"
                    className={`add-card-autocomplete-option block w-full px-3 py-2 text-left text-[13px] transition-colors ${
                      index === autocompleteIndex ? "is-active font-medium" : ""
                    }`}
                    onMouseEnter={() => setAutocompleteIndex(index)}
                    onClick={() => handleAutocompletePick(option)}
                  >
                    {option}
                  </button>
                ))}
              </div>
            ) : null}
          </div>

          <div className="grid gap-3 sm:grid-cols-2">
            <label className={labelClass}>
              {t("addCard.player")}
              <select
                className={selectClass}
                value={selectedPlayer}
                disabled={addLocked}
                onChange={(event) => setPlayerIndex(Number(event.target.value))}
              >
                {players.map((player) => (
                  <option key={player.id} value={player.id}>
                    {player.name}
                  </option>
                ))}
              </select>
            </label>

            <label className={labelClass}>
              {t("addCard.zone")}
              <select
                className={selectClass}
                value={zone}
                disabled={addLocked}
                onChange={(event) => setZone(event.target.value)}
              >
                <option value="hand">{t("zone.hand")}</option>
                <option value="battlefield">{t("zone.battlefield")}</option>
                <option value="graveyard">{t("zone.graveyard")}</option>
                <option value="exile">{t("zone.exile")}</option>
                <option value="library">{t("zone.library")}</option>
                <option value="command">{t("zone.command")}</option>
              </select>
            </label>
          </div>

          <label className="toolbar-checkbox flex items-center gap-2 text-[13px] uppercase tracking-wide">
            <Checkbox
              checked={skipTriggers}
              disabled={addLocked}
              onCheckedChange={(checked) => setSkipTriggers(checked === true)}
              className="h-3.5 w-3.5"
            />
            {t("addCard.skipTriggers")}
          </label>

          <div className="add-card-sheet-footer grid gap-2 sm:grid-cols-2">
            <Button
              type="button"
              variant="secondary"
              size="sm"
              className="stone-pill"
              onClick={closeSheet}
            >
              {t("addCard.cancel")}
            </Button>
            <Button
              type="button"
              size="sm"
              className="add-card-submit ui-primary-action w-full justify-center uppercase tracking-wide"
              onClick={() => handleAdd()}
              disabled={addLocked || !cardName.trim()}
            >
              {t("addCard.submit")}
            </Button>
          </div>
        </div>
      </SheetContent>
    </Sheet>
  );
}
