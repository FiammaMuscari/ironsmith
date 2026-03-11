import { useCallback, useLayoutEffect, useMemo, useRef, useState } from "react";
import { ChevronLeft, ChevronRight } from "lucide-react";
import HoverArtOverlay from "./HoverArtOverlay";
import { useHoveredObjectId } from "@/context/HoverContext";
import { useGame } from "@/context/GameContext";
import { animate, cancelMotion, uiSpring } from "@/lib/motion/anime";
import { playerAccentVars } from "@/lib/player-colors";
import { getVisibleStackObjects } from "@/lib/stack-targets";
import { cn } from "@/lib/utils";

const INSPECTOR_OVERLAY_WIDTH = "clamp(240px, 24vw, 360px)";
const INSPECTOR_INLINE_MIN_WIDTH = 220;
const INSPECTOR_INLINE_FALLBACK_WIDTH = 300;
const INSPECTOR_INLINE_MAX_WIDTH = "min(32vw, 420px)";
const INSPECTOR_INLINE_MAX_WIDTH_PX = 420;
const INLINE_EXPANDED_MIN_WIDTH = 360;
const INLINE_EXPANDED_FALLBACK_WIDTH = 960;
const INLINE_EXPANDED_MAX_WIDTH_PX = 1200;
const INLINE_EXPANDED_MIN_HAND_WIDTH = 168;
const DEFAULT_INSPECTOR_BOTTOM_OFFSET = 8;
const INLINE_EXPANDED_DEFAULT_HEIGHT = 248;
const INLINE_EXPANDED_MIN_HEIGHT = 152;
const INLINE_EXPANDED_SAFE_GAP = 12;
const INLINE_EXPANDED_BOTTOM_GAP = 4;
const INLINE_EXPANDED_RIGHT_BLEED = 8;
const VIEWED_CARD_COMPACT_HEIGHT = 74;

function inspectorBorderStyle(accent) {
  if (!accent) return undefined;
  return {
    ...playerAccentVars(accent),
    borderColor: accent.hex,
    boxShadow: `0 0 0 1px rgba(${accent.rgb}, 0.38), 0 18px 42px rgba(0,0,0,0.24), 0 0 24px rgba(${accent.rgb}, 0.18)`,
  };
}

function clampNumber(value, min, max) {
  return Math.min(Math.max(value, min), max);
}

function objectExistsInState(state, objectId) {
  if (objectId == null) return false;
  const needle = String(objectId);
  const players = state?.players || [];

  for (const player of players) {
    const zones = [
      player?.battlefield || [],
      player?.hand_cards || [],
      player?.graveyard_cards || [],
      player?.exile_cards || [],
      player?.command_cards || [],
    ];
    for (const cards of zones) {
      for (const card of cards) {
        if (String(card?.id) === needle) return true;
        if (Array.isArray(card?.member_ids) && card.member_ids.some((id) => String(id) === needle)) {
          return true;
        }
      }
    }
  }

  for (const entry of getVisibleStackObjects(state)) {
    if (String(entry?.id) === needle) return true;
  }

  if ((state?.viewed_cards?.card_ids || []).some((id) => String(id) === needle)) {
    return true;
  }

  return false;
}

function isViewedCardObject(state, objectId) {
  if (objectId == null) return false;
  const needle = String(objectId);
  return (state?.viewed_cards?.card_ids || []).some((id) => String(id) === needle);
}

function locateObjectInState(state, objectId) {
  if (objectId == null) return null;
  const needle = String(objectId);
  const viewedCards = state?.viewed_cards || null;
  if ((viewedCards?.card_ids || []).some((id) => String(id) === needle)) {
    return {
      side: viewedCards?.visibility === "public" ? "public-view" : "private-view",
      zone: String(viewedCards?.zone || "").toLowerCase(),
      viewVisibility: viewedCards?.visibility === "public" ? "public" : "private",
    };
  }

  const perspective = state?.perspective;
  const players = state?.players || [];
  const zonesByPlayer = [
    ["battlefield", (player) => player?.battlefield || []],
    ["hand", (player) => player?.hand_cards || []],
    ["graveyard", (player) => player?.graveyard_cards || []],
    ["exile", (player) => player?.exile_cards || []],
    ["command", (player) => player?.command_cards || []],
  ];

  for (const player of players) {
    const side = player?.id === perspective ? "self" : "opponent";
    for (const [zone, readCards] of zonesByPlayer) {
      for (const card of readCards(player)) {
        if (String(card?.id) === needle) {
          return { side, zone };
        }
        if (Array.isArray(card?.member_ids) && card.member_ids.some((id) => String(id) === needle)) {
          return { side, zone };
        }
      }
    }
  }

  for (const entry of getVisibleStackObjects(state)) {
    if (String(entry?.id) === needle || String(entry?.inspect_object_id) === needle) {
      return { side: "stack", zone: "stack" };
    }
  }

  return null;
}

function preferredInlinePlacement(location) {
  if (location?.viewVisibility === "private") {
    return { dock: "bottom", side: "right" };
  }
  if (location?.viewVisibility === "public") {
    return { dock: "top", side: "right" };
  }
  return {
    dock: location?.side === "self" && location?.zone !== "stack" ? "top" : "bottom",
    side: "right",
  };
}

function isFocusedDecision(decision) {
  return (
    !!decision
    && decision.kind !== "priority"
    && decision.kind !== "attackers"
    && decision.kind !== "blockers"
  );
}

function decisionReferencesObject(decision, objectId) {
  if (!decision || objectId == null) return false;
  const needle = String(objectId);

  if (decision.kind === "select_objects") {
    return (decision.candidates || []).some((candidate) => String(candidate?.id) === needle);
  }

  if (decision.kind === "targets") {
    return (decision.requirements || []).some((req) =>
      (req.legal_targets || []).some(
        (target) => target?.kind === "object" && String(target?.object) === needle
      )
    );
  }

  if (decision.kind === "select_options") {
    return (decision.options || []).some((opt) => String(opt?.object_id) === needle);
  }

  return false;
}

function ViewedCardsHeader({
  label,
  index,
  total,
  canCycle,
  onCyclePointerDown,
  onCycleClick,
}) {
  const buttonClass = canCycle
    ? "flex h-5 w-5 items-center justify-center rounded border border-[#35506c]/78 bg-[rgba(11,19,29,0.92)] text-[#cfe3fb] transition-colors hover:border-[#4d7093] hover:text-white focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[#80bfff]/70"
    : "flex h-5 w-5 items-center justify-center rounded border border-[#273748]/72 bg-[rgba(11,19,29,0.72)] text-[#5c7591]";
  return (
    <div className="flex h-7 shrink-0 items-center justify-between border-b border-[#314558]/85 bg-[rgba(7,14,22,0.96)] px-1.5">
      <button
        type="button"
        className={buttonClass}
        aria-label={`Show previous ${label.toLowerCase()} card`}
        disabled={!canCycle}
        onPointerDown={(event) => onCyclePointerDown(event, -1)}
        onClick={(event) => onCycleClick(event, -1)}
      >
        <ChevronLeft className="h-3.5 w-3.5" />
      </button>
      <div className="min-w-0 px-2 text-center">
        <div className="text-[10px] font-bold uppercase tracking-[0.14em] text-[#8eb7de]">
          {label}
        </div>
        <div className="text-[10px] text-[#d7e8fb]">
          {index + 1}/{total}
        </div>
      </div>
      <button
        type="button"
        className={buttonClass}
        aria-label={`Show next ${label.toLowerCase()} card`}
        disabled={!canCycle}
        onPointerDown={(event) => onCyclePointerDown(event, 1)}
        onClick={(event) => onCycleClick(event, 1)}
      >
        <ChevronRight className="h-3.5 w-3.5" />
      </button>
    </div>
  );
}

export default function RightRail({
  pinnedObjectId,
  onInspectObject = null,
  suppressFallback = false,
  inspectorBottomOffset = DEFAULT_INSPECTOR_BOTTOM_OFFSET,
  inline = false,
  inlineExpanded = false,
  inlineDockPlacement = "bottom",
  inlineHostSide = "right",
  inlineExpandedSide = "right",
  allowTopInlinePlacement = false,
  forceInlineExpanded = false,
  fullArtInlineExpanded = false,
}) {
  const { state } = useGame();
  const [preferredInlineWidth, setPreferredInlineWidth] = useState(null);
  const [preferredExpandedInlineWidth, setPreferredExpandedInlineWidth] = useState(null);
  const [maxExpandedInlineWidth, setMaxExpandedInlineWidth] = useState(INLINE_EXPANDED_MAX_WIDTH_PX);
  const railRef = useRef(null);
  const compactInspectorRef = useRef(null);
  const expandedInspectorRef = useRef(null);
  const railMotionRef = useRef(null);
  const compactMotionRef = useRef(null);
  const expandedMotionRef = useRef(null);
  const [expandedInlineHeight, setExpandedInlineHeight] = useState(INLINE_EXPANDED_DEFAULT_HEIGHT);
  const [inspectorAccent, setInspectorAccent] = useState(null);
  const hoveredObjectId = useHoveredObjectId();
  const decision = state?.decision || null;
  const stackObjects = getVisibleStackObjects(state);
  const hasStackEntries = stackObjects.length > 0 || (state?.stack_preview || []).length > 0;
  const topStackObject = stackObjects[0];
  const topStackObjectId = topStackObject
    ? String(topStackObject.inspect_object_id ?? topStackObject.id)
    : null;
  const resolvingCastObjectId = state?.stack_size > 0 && topStackObject && !topStackObject.ability_kind
    ? String(topStackObject.inspect_object_id ?? topStackObject.id)
    : null;
  const resolvingCastStableId = resolvingCastObjectId && topStackObject?.stable_id != null
    ? Number(topStackObject.stable_id)
    : null;
  const pinnedInspectorObjectId = pinnedObjectId != null ? String(pinnedObjectId) : null;
  const focusedDecision = isFocusedDecision(decision);
  const pinnedInspectorIsViewedCard = isViewedCardObject(state, pinnedInspectorObjectId);
  const relevantPinnedObjectId = focusedDecision && pinnedInspectorObjectId != null
    ? (
      decisionReferencesObject(decision, pinnedInspectorObjectId) || pinnedInspectorIsViewedCard
        ? pinnedInspectorObjectId
        : null
    )
    : pinnedInspectorObjectId;
  const relevantHoveredObjectId = hoveredObjectId;
  const fallbackDecisionObjectId = suppressFallback ? null : (resolvingCastObjectId ?? topStackObjectId);
  const viewedCardIds = useMemo(
    () => state?.viewed_cards?.card_ids || [],
    [state?.viewed_cards?.card_ids]
  );
  const preferredViewedObjectId = useMemo(() => {
    if (viewedCardIds.length === 0) return null;
    if (isViewedCardObject(state, relevantHoveredObjectId)) return relevantHoveredObjectId;
    if (isViewedCardObject(state, relevantPinnedObjectId)) return relevantPinnedObjectId;
    return String(viewedCardIds[0]);
  }, [relevantHoveredObjectId, relevantPinnedObjectId, state, viewedCardIds]);

  // During focused decision steps, keep the resolving stack object as a fallback.
  // Live hover should always win, even if the current decision does not reference it.
  const decisionLockedObjectId = focusedDecision
    ? (relevantHoveredObjectId ?? relevantPinnedObjectId ?? fallbackDecisionObjectId)
    : null;

  const selectedObjectId = preferredViewedObjectId
    ?? (
      focusedDecision
        ? decisionLockedObjectId
        : (relevantHoveredObjectId ?? relevantPinnedObjectId ?? fallbackDecisionObjectId)
    );
  const validSelectedObjectId = objectExistsInState(state, selectedObjectId)
    ? selectedObjectId
    : null;
  const selectedObjectLocation = useMemo(() => {
    const isCastingSpellFocus = (
      focusedDecision
      && validSelectedObjectId != null
      && resolvingCastObjectId != null
      && String(validSelectedObjectId) === String(resolvingCastObjectId)
      && decision?.player != null
    );
    if (isCastingSpellFocus) {
      return {
        side: Number(decision.player) === Number(state?.perspective) ? "self" : "opponent",
        zone: "casting",
      };
    }
    return locateObjectInState(state, validSelectedObjectId);
  }, [decision?.player, focusedDecision, resolvingCastObjectId, state, validSelectedObjectId]);
  const preferredPlacement = useMemo(
    () => preferredInlinePlacement(selectedObjectLocation),
    [selectedObjectLocation]
  );
  const viewedCardIndex = validSelectedObjectId == null
    ? -1
    : viewedCardIds.findIndex((id) => String(id) === String(validSelectedObjectId));
  const viewedCardsLabel = state?.viewed_cards?.visibility === "public" ? "Revealed" : "Look";
  const hasViewedCards = viewedCardIds.length > 0;
  const effectiveViewedCardIndex = viewedCardIndex >= 0 ? viewedCardIndex : 0;
  const canCycleViewedCards = (
    inline
    && hasViewedCards
    && viewedCardIds.length > 1
    && typeof onInspectObject === "function"
  );
  const resolvedInlineDockPlacement = (
    preferredPlacement.dock === "top" && !allowTopInlinePlacement
      ? "bottom"
      : preferredPlacement.dock
  );
  const suppressDirectResolvingCastInspector =
    !hasStackEntries
    &&
    !focusedDecision
    && pinnedInspectorObjectId == null
    && hoveredObjectId == null
    &&
    validSelectedObjectId != null
    && resolvingCastObjectId != null
    && String(validSelectedObjectId) === String(resolvingCastObjectId);
  const shouldShowInspector = validSelectedObjectId != null && !suppressDirectResolvingCastInspector;
  const shouldShowRail = shouldShowInspector && (
    !inline
    || (
      inlineDockPlacement === resolvedInlineDockPlacement
      && inlineHostSide === preferredPlacement.side
    )
  );
  const shouldRenderExpandedInlineInspector =
    inline
    && shouldShowRail
    && (inlineExpanded || hoveredObjectId != null || pinnedInspectorObjectId != null);
  const useExpandedInlineInspector =
    shouldRenderExpandedInlineInspector
    && (forceInlineExpanded || hoveredObjectId != null || pinnedInspectorObjectId != null);
  const inspectorSuppressStableId = focusedDecision ? null : resolvingCastStableId;
  const inlineWidth = useMemo(() => {
    const preferred = Number.isFinite(preferredInlineWidth)
      ? Math.round(preferredInlineWidth)
      : INSPECTOR_INLINE_FALLBACK_WIDTH;
    return `clamp(${INSPECTOR_INLINE_MIN_WIDTH}px, ${preferred}px, ${INSPECTOR_INLINE_MAX_WIDTH})`;
  }, [preferredInlineWidth]);
  const compactInlineWidthPx = useMemo(() => {
    const preferred = Number.isFinite(preferredInlineWidth)
      ? Math.round(preferredInlineWidth)
      : INSPECTOR_INLINE_FALLBACK_WIDTH;
    return clampNumber(preferred, INSPECTOR_INLINE_MIN_WIDTH, INSPECTOR_INLINE_MAX_WIDTH_PX);
  }, [preferredInlineWidth]);
  const showViewedCardsHeader = inline && hasViewedCards && shouldShowRail;
  const expandedInlineWidth = useMemo(() => {
    const preferred = Number.isFinite(preferredExpandedInlineWidth)
      ? Math.round(preferredExpandedInlineWidth)
      : INLINE_EXPANDED_FALLBACK_WIDTH;
    const minWidth = Math.max(compactInlineWidthPx, INLINE_EXPANDED_MIN_WIDTH);
    const maxWidth = Math.max(minWidth, Math.round(maxExpandedInlineWidth || INLINE_EXPANDED_MAX_WIDTH_PX));
    return clampNumber(preferred, minWidth, maxWidth);
  }, [compactInlineWidthPx, maxExpandedInlineWidth, preferredExpandedInlineWidth]);

  useLayoutEffect(() => {
    const railEl = railRef.current;
    if (!railEl) return undefined;

    cancelMotion(railMotionRef.current);
    railMotionRef.current = animate(railEl, {
      x: shouldShowRail ? 0 : 88,
      opacity: shouldShowRail ? 1 : 0,
      duration: shouldShowRail ? 360 : 280,
      ease: uiSpring({ duration: shouldShowRail ? 360 : 280, bounce: 0.14 }),
    });

    return () => {
      cancelMotion(railMotionRef.current);
      railMotionRef.current = null;
    };
  }, [inline, shouldShowRail]);

  useLayoutEffect(() => {
    const compactEl = compactInspectorRef.current;
    if (!compactEl) return undefined;

    cancelMotion(compactMotionRef.current);
    compactMotionRef.current = animate(compactEl, {
      opacity: useExpandedInlineInspector ? 0 : 1,
      scale: useExpandedInlineInspector ? 0.986 : 1,
      duration: 220,
      ease: "out(3)",
    });

    return () => {
      cancelMotion(compactMotionRef.current);
      compactMotionRef.current = null;
    };
  }, [useExpandedInlineInspector]);

  useLayoutEffect(() => {
    const expandedEl = expandedInspectorRef.current;
    if (!expandedEl) return undefined;

    cancelMotion(expandedMotionRef.current);
    expandedMotionRef.current = animate(expandedEl, {
      opacity: useExpandedInlineInspector ? 1 : 0,
      x: useExpandedInlineInspector ? 0 : 32,
      y: useExpandedInlineInspector ? 0 : 10,
      scale: useExpandedInlineInspector ? 1 : 0.965,
      rotateY: useExpandedInlineInspector ? 0 : -18,
      rotateZ: useExpandedInlineInspector ? 0 : 1.8,
      duration: 420,
      ease: uiSpring({ duration: 420, bounce: 0.12 }),
    });

    return () => {
      cancelMotion(expandedMotionRef.current);
      expandedMotionRef.current = null;
    };
  }, [useExpandedInlineInspector]);

  useLayoutEffect(() => {
    if (!inline) return undefined;
    const railEl = railRef.current;
    if (!railEl) return undefined;

    const workspaceEl = railEl.closest("[data-workspace-shell]") ?? railEl.closest("section");
    const dockEl = railEl.closest("[data-inspector-dock]");
    const handDockEl = dockEl?.querySelector("[data-hand-dock-lane]");
    const stripEl = workspaceEl?.querySelector(".priority-inline-panel");
    const stackEl = workspaceEl?.querySelector("[data-my-zone] [data-inspector-stack-timeline]");
    let rafId = null;

    const measureExpandedLayout = () => {
      const hostRect = (workspaceEl || railEl).getBoundingClientRect();
      const dockRect = dockEl?.getBoundingClientRect?.() || null;
      const stripRect = stripEl?.getBoundingClientRect?.() || null;
      const stackRect = stackEl?.getBoundingClientRect?.() || null;
      const safeTop = inlineDockPlacement === "top"
        ? hostRect.top + INLINE_EXPANDED_SAFE_GAP
        : Math.max(
          stripRect ? stripRect.bottom + INLINE_EXPANDED_SAFE_GAP : hostRect.top + INLINE_EXPANDED_SAFE_GAP,
          stackRect && stackRect.height > 0
            ? stackRect.bottom + INLINE_EXPANDED_SAFE_GAP
            : hostRect.top + INLINE_EXPANDED_SAFE_GAP
        );
      const safeBottom = inlineDockPlacement === "top"
        ? ((dockEl?.getBoundingClientRect?.() || railEl.getBoundingClientRect()).bottom - INLINE_EXPANDED_BOTTOM_GAP)
        : hostRect.bottom - INLINE_EXPANDED_BOTTOM_GAP;
      const availableHeight = Math.max(0, Math.floor(safeBottom - safeTop));
      const minimumHeight = Math.min(INLINE_EXPANDED_MIN_HEIGHT, availableHeight);
      const nextHeight = Math.max(
        minimumHeight,
        Math.min(INLINE_EXPANDED_DEFAULT_HEIGHT, availableHeight)
      );

      setExpandedInlineHeight((currentHeight) => (
        Math.abs(currentHeight - nextHeight) >= 1 ? nextHeight : currentHeight
      ));

      const dockGap = dockEl
        ? parseFloat(getComputedStyle(dockEl).columnGap || getComputedStyle(dockEl).gap || "0")
        : 0;
      const availableWidth = dockRect
        ? (
          inlineDockPlacement === "top"
            ? dockRect.width
            : dockRect.width - INLINE_EXPANDED_MIN_HAND_WIDTH - dockGap
        )
        : INLINE_EXPANDED_MAX_WIDTH_PX;
      const nextMaxWidth = Math.max(
        Math.max(compactInlineWidthPx, INLINE_EXPANDED_MIN_WIDTH),
        Math.min(Math.floor(availableWidth), INLINE_EXPANDED_MAX_WIDTH_PX)
      );
      setMaxExpandedInlineWidth((currentWidth) => (
        Math.abs(currentWidth - nextMaxWidth) >= 1 ? nextMaxWidth : currentWidth
      ));
    };

    const scheduleMeasure = () => {
      if (rafId != null) cancelAnimationFrame(rafId);
      rafId = requestAnimationFrame(() => {
        rafId = null;
        measureExpandedLayout();
      });
    };

    scheduleMeasure();

    const observer = new ResizeObserver(scheduleMeasure);
    observer.observe(railEl);
    if (workspaceEl) observer.observe(workspaceEl);
    if (dockEl) observer.observe(dockEl);
    if (handDockEl) observer.observe(handDockEl);
    if (stripEl) observer.observe(stripEl);
    if (stackEl) observer.observe(stackEl);
    window.addEventListener("resize", scheduleMeasure);

    return () => {
      if (rafId != null) cancelAnimationFrame(rafId);
      observer.disconnect();
      window.removeEventListener("resize", scheduleMeasure);
    };
  }, [compactInlineWidthPx, inline, inlineDockPlacement, shouldRenderExpandedInlineInspector, shouldShowRail]);

  const containerStyle = useMemo(
    () => (inline
      ? {
        width: shouldShowRail
          ? (useExpandedInlineInspector ? `${expandedInlineWidth}px` : inlineWidth)
          : "0px",
        height: shouldShowRail && showViewedCardsHeader && !useExpandedInlineInspector
          ? `${VIEWED_CARD_COMPACT_HEIGHT}px`
          : undefined,
      }
      : {
        width: INSPECTOR_OVERLAY_WIDTH,
        top: 8,
        bottom: inspectorBottomOffset,
      }),
    [
      expandedInlineWidth,
      inline,
      inlineWidth,
      inspectorBottomOffset,
      showViewedCardsHeader,
      shouldShowRail,
      useExpandedInlineInspector,
    ]
  );
  const expandedInlineShellOffset = inlineExpandedSide === "left"
    ? { left: `-${INLINE_EXPANDED_RIGHT_BLEED}px`, right: "auto", transformOrigin: "bottom left" }
    : { left: "auto", right: `-${INLINE_EXPANDED_RIGHT_BLEED}px`, transformOrigin: "bottom right" };
  const cycleViewedCard = useCallback((direction) => {
    if (!canCycleViewedCards) return;
    const nextIndex = (
      effectiveViewedCardIndex + direction + viewedCardIds.length
    ) % viewedCardIds.length;
    const nextObjectId = viewedCardIds[nextIndex];
    if (nextObjectId == null) return;
    onInspectObject(nextObjectId);
  }, [canCycleViewedCards, effectiveViewedCardIndex, onInspectObject, viewedCardIds]);

  const handleCyclePointerDown = useCallback((event, direction) => {
    event.preventDefault();
    event.stopPropagation();
    cycleViewedCard(direction);
  }, [cycleViewedCard]);

  const handleCycleClick = useCallback((event, direction) => {
    if (event.detail !== 0) return;
    event.preventDefault();
    event.stopPropagation();
    cycleViewedCard(direction);
  }, [cycleViewedCard]);

  return (
    <aside
      ref={railRef}
      className={cn(
        inline
          ? "pointer-events-none relative h-full self-end shrink-0 overflow-visible transition-[width] duration-320 ease-[cubic-bezier(0.22,1,0.36,1)]"
          : "pointer-events-none absolute right-2 z-40"
      )}
      style={containerStyle}
      aria-hidden={!shouldShowRail}
    >
      <div className={cn("relative h-full min-h-0", inline ? "overflow-visible" : "overflow-hidden")}>
        <div
          ref={compactInspectorRef}
          className={cn(
            "h-full overflow-hidden border border-[#2a3647]/70 bg-transparent shadow-[0_18px_42px_rgba(0,0,0,0.24)]",
            inline
              ? (inlineExpandedSide === "left" ? "rounded-l rounded-r-sm" : "rounded-r rounded-l-sm")
              : "rounded",
            shouldShowRail ? "pointer-events-auto" : "pointer-events-none"
          )}
          style={inspectorBorderStyle(inspectorAccent)}
        >
          <div className="flex h-full min-h-0 flex-col overflow-hidden">
            {showViewedCardsHeader && (
              <ViewedCardsHeader
                label={viewedCardsLabel}
                index={effectiveViewedCardIndex}
                total={viewedCardIds.length}
                canCycle={canCycleViewedCards}
                onCyclePointerDown={handleCyclePointerDown}
                onCycleClick={handleCycleClick}
              />
            )}
            <div className="relative min-h-0 flex-1 overflow-hidden">
              <HoverArtOverlay
                objectId={shouldShowRail ? validSelectedObjectId : null}
                suppressStableId={inspectorSuppressStableId}
                compact={inline}
                onPreferredWidthChange={inline ? setPreferredInlineWidth : null}
                onInspectorAccentChange={useExpandedInlineInspector ? null : setInspectorAccent}
              />
            </div>
          </div>
        </div>
        {shouldRenderExpandedInlineInspector && (
          <div
            ref={expandedInspectorRef}
            className={cn(
              "hand-inspector-inline-shell absolute bottom-0 overflow-hidden border border-[#2a3647]/75 bg-[rgba(8,12,18,0.94)]",
              useExpandedInlineInspector ? "is-open" : "is-closed"
            )}
            style={{
              width: "100%",
              height: `${expandedInlineHeight}px`,
              ...expandedInlineShellOffset,
              ...inspectorBorderStyle(inspectorAccent),
            }}
          >
            <div className="flex h-full min-h-0 flex-col overflow-hidden">
              {showViewedCardsHeader && (
                <ViewedCardsHeader
                  label={viewedCardsLabel}
                  index={effectiveViewedCardIndex}
                  total={viewedCardIds.length}
                  canCycle={canCycleViewedCards}
                  onCyclePointerDown={handleCyclePointerDown}
                  onCycleClick={handleCycleClick}
                />
              )}
              <div className="min-h-0 flex-1 overflow-hidden">
                <HoverArtOverlay
                  objectId={shouldShowRail ? validSelectedObjectId : null}
                  suppressStableId={inspectorSuppressStableId}
                  displayMode={fullArtInlineExpanded ? "full-art" : "inspector"}
                  availableInspectorWidth={expandedInlineWidth}
                  availableInspectorHeight={expandedInlineHeight}
                  onPreferredInspectorWidthChange={
                    fullArtInlineExpanded ? null : setPreferredExpandedInlineWidth
                  }
                  onInspectorAccentChange={useExpandedInlineInspector ? setInspectorAccent : null}
                />
              </div>
            </div>
          </div>
        )}
      </div>
    </aside>
  );
}
