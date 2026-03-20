const CARD_ASPECT_RATIO = 124 / 96;
const MOBILE_MIN_CARD_HEIGHT = 24;
const MOBILE_MAX_CARD_HEIGHT = 82;
const MOBILE_BATTLEFIELD_SIDE_PADDING_PX = 8;
const MOBILE_ROW_GAP_PX = 6;
const MOBILE_SECTION_GAP_PX = 6;
const MOBILE_BOTTOM_PEEK_HEIGHT_PX = 46;
const MOBILE_BOTTOM_BAR_HEIGHT_PX = 0;
const MOBILE_TOP_BUFFER_PX = 2;
const MOBILE_CONTROL_BAND_MIN_HEIGHT_PX = 28;
const MOBILE_CONTROL_BAND_MAX_HEIGHT_PX = 72;
const MOBILE_TOP_STATUS_FALLBACK_PX = 30;
const MOBILE_BACK_ROW_VISIBLE_RATIO = 0.78;

function clamp(value, min, max) {
  return Math.min(max, Math.max(min, value));
}

export function solveMobileBattleLayout({
  viewportWidth = 0,
  viewportHeight = 0,
  safeAreaTop = 0,
  safeAreaBottom = 0,
  topBandHeight = 0,
  controlBandHeight = 0,
  collapsedHandRailHeight = MOBILE_BOTTOM_PEEK_HEIGHT_PX,
  opponentFrontCount = 0,
  opponentBackCount = 0,
  selfFrontCount = 0,
  selfBackCount = 0,
}) {
  const width = Math.max(1, Math.floor(viewportWidth || 0));
  const height = Math.max(1, Math.floor(viewportHeight || 0));
  const sidePadding = width <= 360 ? 6 : MOBILE_BATTLEFIELD_SIDE_PADDING_PX;
  const rowGap = MOBILE_ROW_GAP_PX;
  const sectionGap = MOBILE_SECTION_GAP_PX;
  const topStatusHeight = Math.max(
    MOBILE_TOP_STATUS_FALLBACK_PX,
    Math.ceil((topBandHeight || 0) + MOBILE_TOP_BUFFER_PX)
  );
  const normalizedControlBandHeight = controlBandHeight > 0
    ? clamp(
      Math.ceil(controlBandHeight || 0),
      MOBILE_CONTROL_BAND_MIN_HEIGHT_PX,
      MOBILE_CONTROL_BAND_MAX_HEIGHT_PX
    )
    : 0;
  const bottomPeekHeight = Math.max(
    0,
    Math.ceil(collapsedHandRailHeight || 0)
  );
  const bottomBandHeight = bottomPeekHeight > 0
    ? Math.max(
      MOBILE_BOTTOM_BAR_HEIGHT_PX,
      bottomPeekHeight
    )
    : 0;
  const maxColumns = Math.max(
    1,
    opponentFrontCount,
    opponentBackCount,
    selfFrontCount,
    selfBackCount
  );
  const usableWidth = Math.max(1, width - (sidePadding * 2));
  const widthLimitedCard = Math.floor(
    (usableWidth - (Math.max(0, maxColumns - 1) * rowGap)) / maxColumns
  );
  const availableBattlefieldHeight = Math.max(
    MOBILE_MIN_CARD_HEIGHT,
    height
      - safeAreaTop
      - safeAreaBottom
      - topStatusHeight
      - normalizedControlBandHeight
      - bottomBandHeight
      - (sectionGap * 3)
  );
  const battlefieldFixedRowGaps = rowGap * 2;
  const heightLimitedCard = Math.floor(
    (availableBattlefieldHeight - battlefieldFixedRowGaps)
      / (3 + MOBILE_BACK_ROW_VISIBLE_RATIO)
  );
  const cardHeight = clamp(
    Math.min(
      MOBILE_MAX_CARD_HEIGHT,
      heightLimitedCard,
      Math.floor(widthLimitedCard / CARD_ASPECT_RATIO)
    ),
    MOBILE_MIN_CARD_HEIGHT,
    MOBILE_MAX_CARD_HEIGHT
  );
  const cardWidth = Math.max(
    1,
    Math.floor(cardHeight * CARD_ASPECT_RATIO)
  );
  const selfBackVisibleHeight = Math.max(
    1,
    Math.ceil(cardHeight * MOBILE_BACK_ROW_VISIBLE_RATIO)
  );
  const opponentBandHeight = (cardHeight * 2) + rowGap;
  const selfBandHeight = cardHeight + rowGap + selfBackVisibleHeight;
  const totalHeight =
    topStatusHeight
    + sectionGap
    + opponentBandHeight
    + sectionGap
    + normalizedControlBandHeight
    + sectionGap
    + selfBandHeight
    + bottomBandHeight
    + safeAreaTop
    + safeAreaBottom;

  return {
    viewportWidth: width,
    viewportHeight: height,
    safeAreaTop,
    safeAreaBottom,
    sidePadding,
    rowGap,
    sectionGap,
    topStatusHeight,
    controlBandHeight: normalizedControlBandHeight,
    bottomPeekHeight,
    bottomBandHeight,
    cardWidth,
    cardHeight,
    opponentBandHeight,
    selfFrontHeight: cardHeight,
    selfBackVisibleHeight,
    selfBackVisibleRatio: MOBILE_BACK_ROW_VISIBLE_RATIO,
    compactMode: height <= 320,
    totalHeight,
    fitsViewport: totalHeight <= height,
  };
}
