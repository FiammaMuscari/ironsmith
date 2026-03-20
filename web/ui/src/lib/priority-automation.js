import { isCombatPhase, isEndingPhase, isMainPhase } from "./constants.js";

export const LOCAL_STACK_MANUAL_HOLD_REASON = "manual stack resolve";

export function priorityHoldReason({
  autoPassEnabled,
  holdRule,
  decision,
  currentState,
  perspectiveMode = "any",
  requireNonEmptyStack = false,
  manualResolveOnLocalStack = false,
}) {
  if (!autoPassEnabled) return "auto-pass disabled";
  if (!decision || decision.kind !== "priority") return "not a priority decision";

  const perspective = currentState?.perspective;
  if (perspectiveMode === "local" && decision.player !== perspective) return "not local priority";
  if (perspectiveMode === "opponent" && decision.player === perspective) return "not opponent priority";

  const stackSize = Number(currentState?.stack_size || 0);
  if (manualResolveOnLocalStack && perspectiveMode === "local" && stackSize > 0) {
    return LOCAL_STACK_MANUAL_HOLD_REASON;
  }
  if (requireNonEmptyStack && stackSize <= 0) return "stack empty";

  if (holdRule === "never") return null;
  if (holdRule === "always") return "always hold";
  if (holdRule === "stack" && stackSize > 0) return "stack non-empty";
  if (holdRule === "main" && isMainPhase(currentState?.phase)) return "main phase";
  if (holdRule === "combat" && isCombatPhase(currentState?.phase)) return "combat phase";
  if (holdRule === "ending" && isEndingPhase(currentState?.phase)) return "ending phase";
  if (holdRule === "if_actions") {
    const hasNonPass = (decision.actions || []).some((action) => action.kind !== "pass_priority");
    if (hasNonPass) {
      return perspectiveMode === "opponent"
        ? "opponent has playable actions"
        : "playable actions available";
    }
  }

  return null;
}
