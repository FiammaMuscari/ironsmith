export const PAPER_FRONT_LANES = ["creatures", "enchantments", "planeswalkers", "battles"];
export const PAPER_BACK_LANES = ["artifacts", "lands", "other"];
export const ALL_PAPER_LANES = [...PAPER_FRONT_LANES, ...PAPER_BACK_LANES];

export function normalizeBattlefieldLane(lane) {
  const normalized = String(lane || "").toLowerCase();
  return ALL_PAPER_LANES.includes(normalized) ? normalized : "other";
}

export function partitionBattlefieldCards(cards = []) {
  const frontCards = [];
  const backCards = [];

  for (const card of cards) {
    const lane = normalizeBattlefieldLane(card?.lane);
    if (PAPER_FRONT_LANES.includes(lane)) {
      frontCards.push(card);
    } else {
      backCards.push(card);
    }
  }

  return {
    frontCards,
    backCards,
    frontCount: frontCards.length,
    backCount: backCards.length,
  };
}
