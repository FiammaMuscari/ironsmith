import { useGame } from "@/context/GameContext";
import useViewportLayout from "@/hooks/useViewportLayout";
import { isEffectOrderingDecision } from "@/lib/effect-ordering";

// Both desktop appearances ("Compact artwork" and "Full cards") show the
// stack as a pile of rendered cards over the board (CardStackPile) instead of
// the scrolling rail. Phones keep their rails, and ordering triggers or
// replacements keeps the rail too: its reorder controls have no place on a
// pile.
export default function useCardStackMode() {
  const { state } = useGame();
  const { nonDesktopViewport } = useViewportLayout();
  return !nonDesktopViewport
    && !isEffectOrderingDecision(state?.decision);
}
