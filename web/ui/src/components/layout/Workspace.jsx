import { useState, useEffect } from "react";
import { useGame } from "@/context/GameContext";
import { useDragActions } from "@/context/DragContext";
import TableCore from "@/components/board/TableCore";
import RightRail from "@/components/right-rail/RightRail";
import HandZone from "@/components/board/HandZone";
import StackPanel from "@/components/right-rail/StackPanel";
import DragOverlay from "@/components/overlays/DragOverlay";
import CastParticles from "@/components/overlays/CastParticles";
import ArrowOverlay from "@/components/overlays/ArrowOverlay";

export default function Workspace({ zoneView, deckLoadingMode, onLoadDecks, onCancelDeckLoading }) {
  const [selectedObjectId, setSelectedObjectId] = useState(null);
  const { state, dispatch } = useGame();
  const { endDrag } = useDragActions();

  const players = state?.players || [];
  const perspective = state?.perspective;
  const me = players.find((p) => p.id === perspective) || players[0];

  // Handle drag drop — if user drops on the battlefield area, dispatch the action
  useEffect(() => {
    const onPointerUp = (e) => {
      const ds = endDrag();
      if (!ds || !ds.actions || ds.actions.length === 0) return;

      // Check if dropped over the table area (anywhere above the hand)
      const el = document.elementFromPoint(e.clientX, e.clientY);
      const isOverTable = el && (
        el.closest("[data-drop-zone]") ||
        el.closest(".table-gradient") ||
        el.closest(".board-zone-bg")
      );

      if (isOverTable) {
        // If single action, dispatch immediately. If multiple, use first.
        const action = ds.actions[0];
        dispatch(
          { type: "priority_action", action_index: action.index },
          action.label
        );
        // Spawn particles at drop point
        window.__castParticles?.(e.clientX, e.clientY, ds.glowKind || "spell");
      }
    };

    const onPointerCancel = () => {
      endDrag();
    };

    const onWindowBlur = () => {
      endDrag();
    };

    document.addEventListener("pointerup", onPointerUp);
    document.addEventListener("pointercancel", onPointerCancel);
    window.addEventListener("blur", onWindowBlur);
    return () => {
      document.removeEventListener("pointerup", onPointerUp);
      document.removeEventListener("pointercancel", onPointerCancel);
      window.removeEventListener("blur", onWindowBlur);
    };
  }, [endDrag, dispatch]);

  return (
    <section
      className="grid gap-2 min-h-0 h-full"
      style={{
        gridTemplateColumns: "clamp(143px,12vw,195px) minmax(0,1fr)",
        gridTemplateRows: "minmax(0,1fr) 140px",
      }}
    >
      <DragOverlay />
      <CastParticles />
      <ArrowOverlay />
      <RightRail pinnedObjectId={selectedObjectId} />
      <TableCore
        selectedObjectId={selectedObjectId}
        onInspect={setSelectedObjectId}
        zoneView={zoneView}
        deckLoadingMode={deckLoadingMode}
        onLoadDecks={onLoadDecks}
        onCancelDeckLoading={onCancelDeckLoading}
      />
      <div className="col-span-2 flex min-h-0 h-full overflow-visible">
        <div className="flex-1 min-w-0 h-full overflow-visible">
          <HandZone player={me} selectedObjectId={selectedObjectId} onInspect={setSelectedObjectId} />
        </div>
        <div className="shrink-0 h-full rail-gradient rounded overflow-hidden" style={{ width: "clamp(160px, 20vw, 280px)" }}>
          <StackPanel onInspect={setSelectedObjectId} />
        </div>
      </div>
    </section>
  );
}
