import { useEffect, useMemo, useState } from "react";
import { useGame } from "@/context/GameContext";
import InspectorStackTimeline from "./InspectorStackTimeline";
import { cn } from "@/lib/utils";

const STACK_RAIL_WIDTH = "clamp(220px, 24vw, 320px)";
const STACK_LEAVE_ANIMATION_MS = 360;

export default function StackTimelineRail({
  selectedObjectId = null,
  onInspectObject = null,
}) {
  const { state } = useGame();
  const decision = state?.decision || null;
  const canAct = !!decision && decision.player === state?.perspective;
  const stackObjects = state?.stack_objects || [];
  const stackPreview = state?.stack_preview || [];
  const rawStackEntryCount = Math.max(stackObjects.length, stackPreview.length);
  const [displayStackEntryCount, setDisplayStackEntryCount] = useState(rawStackEntryCount);

  useEffect(() => {
    if (rawStackEntryCount === displayStackEntryCount) return undefined;

    if (rawStackEntryCount > displayStackEntryCount) {
      const timeout = setTimeout(() => {
        setDisplayStackEntryCount(rawStackEntryCount);
      }, 0);
      return () => clearTimeout(timeout);
    }

    const timeout = setTimeout(() => {
      setDisplayStackEntryCount(rawStackEntryCount);
    }, STACK_LEAVE_ANIMATION_MS);
    return () => clearTimeout(timeout);
  }, [rawStackEntryCount, displayStackEntryCount]);

  const shouldShowRail = displayStackEntryCount > 0;
  const containerStyle = useMemo(
    () => ({ width: shouldShowRail ? STACK_RAIL_WIDTH : "0px" }),
    [shouldShowRail]
  );

  return (
    <aside
      className={cn(
        "pointer-events-none relative h-full shrink-0 overflow-hidden transition-[width,transform,opacity] duration-220 ease-out",
        shouldShowRail ? "translate-x-0 opacity-100" : "-translate-x-[110%] opacity-0"
      )}
      style={containerStyle}
      aria-hidden={!shouldShowRail}
    >
      <div className={cn("h-full overflow-hidden", shouldShowRail ? "pointer-events-auto" : "pointer-events-none")}>
        {shouldShowRail && (
          <InspectorStackTimeline
            embedded
            decision={decision}
            canAct={canAct}
            stackObjects={stackObjects}
            stackPreview={stackPreview}
            selectedObjectId={selectedObjectId}
            onInspectObject={onInspectObject}
          />
        )}
      </div>
    </aside>
  );
}
