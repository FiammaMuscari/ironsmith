import { createContext, useContext, useState, useCallback, useMemo } from "react";

const HoverStateContext = createContext(undefined);
const HoverActionsContext = createContext(undefined);

export function HoverProvider({ children }) {
  const [hoveredObjectId, setHoveredObjectId] = useState(null);

  const hoverCard = useCallback((objectId) => {
    setHoveredObjectId(objectId != null ? String(objectId) : null);
  }, []);

  const clearHover = useCallback(() => {
    setHoveredObjectId(null);
  }, []);

  const actions = useMemo(
    () => ({ hoverCard, clearHover }),
    [hoverCard, clearHover]
  );

  return (
    <HoverStateContext.Provider value={hoveredObjectId}>
      <HoverActionsContext.Provider value={actions}>
        {children}
      </HoverActionsContext.Provider>
    </HoverStateContext.Provider>
  );
}

export function useHoveredObjectId() {
  const hoveredObjectId = useContext(HoverStateContext);
  if (hoveredObjectId === undefined) {
    throw new Error("useHoveredObjectId must be inside HoverProvider");
  }
  return hoveredObjectId;
}

export function useHoverActions() {
  const ctx = useContext(HoverActionsContext);
  if (!ctx) throw new Error("useHoverActions must be inside HoverProvider");
  return ctx;
}

export function useHover() {
  const hoveredObjectId = useHoveredObjectId();
  const { hoverCard, clearHover } = useHoverActions();
  return { hoveredObjectId, hoverCard, clearHover };
}
