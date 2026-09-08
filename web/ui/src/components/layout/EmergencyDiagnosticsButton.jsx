import { useRef, useState } from "react";
import { Bug, Check, LoaderCircle } from "lucide-react";
import { useGame } from "@/context/GameContext";
import { copyTextToClipboard } from "@/lib/clipboard";
import { exportDiagnostics, getDiagnosticsSnapshot } from "@/lib/action-diagnostics";
import {
  diagnosticSignals,
  readEngineDiagnostics,
  runtimeEnvironmentDiagnostics,
} from "@/lib/engine-diagnostics";

export default function EmergencyDiagnosticsButton() {
  const { game, multiplayer, state, setStatus } = useGame();
  const [status, setButtonStatus] = useState("idle");
  const resetTimerRef = useRef(null);

  const copyDiagnostics = async () => {
    if (status === "copying") return;
    setButtonStatus("copying");
    const snapshot = getDiagnosticsSnapshot();
    const engine = await readEngineDiagnostics(game, 250);
    const payload = exportDiagnostics({
      captureReason: "emergency-button",
      signals: diagnosticSignals(snapshot, engine),
      environment: runtimeEnvironmentDiagnostics(),
      engine,
      game: {
        phase: state?.phase,
        step: state?.step,
        turn: state?.turn,
        decision: state?.decision?.kind,
        priorityPlayer: state?.priority_player,
        battlefieldSize: Array.isArray(state?.battlefield) ? state.battlefield.length : null,
        stackSize: Array.isArray(state?.stack) ? state.stack.length : null,
      },
      multiplayer: {
        mode: multiplayer?.mode,
        role: multiplayer?.role,
        lastAppliedSequence: multiplayer?.lastAppliedSequence,
        submittingAction: multiplayer?.submittingAction,
        peerWait: multiplayer?.peerWait,
        connectionWarnings: multiplayer?.connectionWarnings,
        matchClock: multiplayer?.matchClock,
        players: multiplayer?.players,
      },
    });
    const copied = await copyTextToClipboard(`${JSON.stringify(payload, null, 2)}\n`);
    setButtonStatus(copied ? "copied" : "failed");
    setStatus?.(copied ? "Diagnostics copied" : "Could not copy diagnostics", !copied);
    window.clearTimeout(resetTimerRef.current);
    resetTimerRef.current = window.setTimeout(() => setButtonStatus("idle"), 1800);
  };

  const Icon = status === "copied" ? Check : status === "copying" ? LoaderCircle : Bug;
  const label = status === "copied" ? "Copied" : status === "copying" ? "Copying…" : status === "failed" ? "Retry debug" : "Copy debug";

  return (
    <button
      type="button"
      onClick={() => void copyDiagnostics()}
      className="fixed bottom-3 right-3 inline-flex items-center gap-1.5 rounded-md border border-amber-300/70 bg-slate-950/95 px-3 py-2 text-xs font-semibold text-amber-100 shadow-2xl backdrop-blur hover:bg-slate-900 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-amber-300"
      style={{ zIndex: 2147483647 }}
      aria-label="Copy emergency game diagnostics"
      title="Copy engine, browser and connection diagnostics"
      data-copy-status={status}
    >
      <Icon className={`size-4 ${status === "copying" ? "animate-spin" : ""}`} aria-hidden="true" />
      {label}
    </button>
  );
}
