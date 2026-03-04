import DecisionPanel from "@/components/left-rail/DecisionPanel";
import InspectorPanel from "./InspectorPanel";
import { useHoveredObjectId } from "@/context/HoverContext";

export default function RightRail({ pinnedObjectId }) {
  const hoveredObjectId = useHoveredObjectId();
  const selectedObjectId = hoveredObjectId ?? pinnedObjectId;

  return (
    <aside className="rail-gradient rounded flex flex-col min-h-0 overflow-hidden">
      <div className="flex flex-col flex-1 min-h-0">
        <DecisionPanel />
        <div className="flex-1 min-h-0 overflow-hidden">
          <InspectorPanel selectedObjectId={selectedObjectId} pinnedObjectId={pinnedObjectId} />
        </div>
      </div>
    </aside>
  );
}
