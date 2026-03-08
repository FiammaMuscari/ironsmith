export function getCardElement(objectId) {
  return document.querySelector(
    `[data-arrow-anchor="stack"][data-object-id="${objectId}"], .game-card[data-object-id="${objectId}"]`
  );
}

export function getCardRect(objectId) {
  const el = getCardElement(objectId);
  return el ? el.getBoundingClientRect() : null;
}

export function getPlayerTargetRect(playerIndex) {
  const el =
    document.querySelector(`[data-player-target-name="${playerIndex}"]`) ||
    document.querySelector(`[data-player-target="${playerIndex}"]`);
  return el ? el.getBoundingClientRect() : null;
}

export function centerOf(rect) {
  return { x: rect.left + rect.width / 2, y: rect.top + rect.height / 2 };
}
