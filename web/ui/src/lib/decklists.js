export const MATCH_FORMAT_NORMAL = "normal";
export const MATCH_FORMAT_COMMANDER = "commander";
export const LOBBY_DECK_SIZE = 60;
export const COMMANDER_DECK_SIZE = 99;
export const PARTNER_DECK_SIZE = 98;

const MAIN_DECK_HEADER = /^Deck$/i;
const COMMANDER_HEADER = /^(Commander|Commanders)$/i;
const EXTRA_DECK_HEADER = /^(Sideboard|Companion|Maybeboard)$/i;
const CARD_LINE = /^(\d+)x?\s+(.+)$/;

function normalizeCardName(raw) {
  return String(raw || "")
    .replace(/\s*\([A-Z0-9]+\)\s*\d*\*?\s*$/, "")
    .trim();
}

export function parseDeckList(text) {
  const cards = [];
  let parsingMainDeck = true;

  for (const line of String(text || "").split("\n")) {
    const trimmed = line.trim();
    if (!trimmed || trimmed.startsWith("//") || trimmed.startsWith("#")) continue;

    if (MAIN_DECK_HEADER.test(trimmed)) {
      parsingMainDeck = true;
      continue;
    }

    if (COMMANDER_HEADER.test(trimmed) || EXTRA_DECK_HEADER.test(trimmed)) {
      parsingMainDeck = false;
      continue;
    }

    if (!parsingMainDeck) continue;

    const match = trimmed.match(CARD_LINE);
    if (!match) continue;

    const count = parseInt(match[1], 10);
    const name = normalizeCardName(match[2]);
    for (let i = 0; i < count; i += 1) {
      cards.push(name);
    }
  }

  return cards;
}

export function parseCommanderList(text) {
  const cards = [];

  for (const line of String(text || "").split("\n")) {
    const trimmed = line.trim();
    if (!trimmed || trimmed.startsWith("//") || trimmed.startsWith("#")) continue;
    if (
      MAIN_DECK_HEADER.test(trimmed)
      || COMMANDER_HEADER.test(trimmed)
      || EXTRA_DECK_HEADER.test(trimmed)
    ) {
      continue;
    }

    const match = trimmed.match(CARD_LINE);
    if (match) {
      const count = parseInt(match[1], 10);
      const name = normalizeCardName(match[2]);
      for (let i = 0; i < count; i += 1) {
        cards.push(name);
      }
      continue;
    }

    cards.push(normalizeCardName(trimmed));
  }

  return cards;
}

export function normalizeMatchFormat(raw) {
  return raw === MATCH_FORMAT_COMMANDER
    ? MATCH_FORMAT_COMMANDER
    : MATCH_FORMAT_NORMAL;
}

export function evaluateLobbyDeckSubmission(format, deck, commanders = []) {
  const normalizedFormat = normalizeMatchFormat(format);
  const deckCount = Array.isArray(deck) ? deck.length : 0;
  const commanderCount = Array.isArray(commanders) ? commanders.length : 0;

  if (normalizedFormat === MATCH_FORMAT_COMMANDER) {
    const requiredDeckCount = commanderCount === 2 ? PARTNER_DECK_SIZE : COMMANDER_DECK_SIZE;
    const ready =
      (commanderCount === 1 || commanderCount === 2)
      && deckCount === requiredDeckCount;
    return {
      ready,
      deckCount,
      commanderCount,
      requiredDeckCount,
    };
  }

  return {
    ready: deckCount === LOBBY_DECK_SIZE,
    deckCount,
    commanderCount,
    requiredDeckCount: LOBBY_DECK_SIZE,
  };
}

export function isLobbyDeckReady(deck) {
  return Array.isArray(deck) && deck.length === LOBBY_DECK_SIZE;
}
