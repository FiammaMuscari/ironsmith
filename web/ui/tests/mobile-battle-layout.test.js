import test from "node:test";
import assert from "node:assert/strict";
import { solveMobileBattleLayout } from "../src/lib/mobile-battle-layout.js";

const HEIGHTS = [390, 340, 320, 300, 280];

for (const height of HEIGHTS) {
  test(`mobile battle layout fits viewport at ${height}px height`, () => {
    const layout = solveMobileBattleLayout({
      viewportWidth: 844,
      viewportHeight: height,
      topBandHeight: 42,
      controlBandHeight: 38,
      collapsedHandRailHeight: 58,
      opponentFrontCount: 7,
      opponentBackCount: 7,
      selfFrontCount: 7,
      selfBackCount: 7,
    });

    assert.equal(layout.fitsViewport, true);
    assert.ok(layout.totalHeight <= layout.viewportHeight);
    assert.ok(layout.selfFrontHeight === layout.cardHeight);
    assert.ok(layout.selfBackVisibleHeight >= Math.floor(layout.cardHeight * 0.78));
    assert.equal(layout.selfBackVisibleRatio, 0.78);

    const usableWidth = layout.viewportWidth - (layout.sidePadding * 2);
    const rowWidth = (layout.cardWidth * 7) + (layout.rowGap * 6);
    assert.ok(rowWidth <= usableWidth);
  });
}

test("mobile battle layout preserves mixed battlefield rows without overlap", () => {
  const layout = solveMobileBattleLayout({
    viewportWidth: 844,
    viewportHeight: 320,
    topBandHeight: 44,
    controlBandHeight: 72,
    collapsedHandRailHeight: 58,
    opponentFrontCount: 5,
    opponentBackCount: 7,
    selfFrontCount: 4,
    selfBackCount: 6,
  });

  assert.equal(layout.fitsViewport, true);
  assert.ok(layout.cardWidth > 0);
  assert.ok(layout.cardHeight > 0);
  assert.ok(layout.opponentBandHeight >= (layout.cardHeight * 2));
  assert.ok(layout.bottomBandHeight >= 46);
});
