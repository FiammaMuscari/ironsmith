import useUiText from "@/i18n/useUiText";
import { useRef } from "react";
import { Hand } from "lucide-react";
import { useGame } from "@/context/GameContext";

export default function PriorityHoldControl({ compact = false }) {
  const ui = useUiText();
  const { holdRule, setHoldRule } = useGame();
  const previousRule = useRef("never");
  const holding = holdRule === "always";
  const label = ui(holding ? "Holding priority" : "Hold priority");

  return (
    <button
      type="button"
      className={`player-priority-hold${compact ? " player-priority-hold--compact" : ""}`}
      aria-pressed={holding}
      aria-label={label}
      title={compact ? undefined : ui(holding
        ? "Automatic priority passing is paused. Click to restore your previous hold setting."
        : "Hold priority until turned off, including after casting your own spells. Enable before casting.")}
      data-tooltip={compact ? label : undefined}
      onPointerDown={(event) => event.stopPropagation()}
      onClick={(event) => {
        event.stopPropagation();
        if (holding) {
          setHoldRule(previousRule.current);
        } else {
          previousRule.current = holdRule || "never";
          setHoldRule("always");
        }
      }}
    >
      <Hand size={13} aria-hidden="true" />
      <span className={compact ? "sr-only" : undefined}>{label}</span>
    </button>
  );
}
