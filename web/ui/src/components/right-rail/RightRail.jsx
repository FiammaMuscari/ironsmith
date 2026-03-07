import { useLayoutEffect, useMemo, useRef, useState } from "react";
import HoverArtOverlay from "./HoverArtOverlay";
import { useHoveredObjectId } from "@/context/HoverContext";
import { useGame } from "@/context/GameContext";
import { animate, cancelMotion, uiSpring } from "@/lib/motion/anime";
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

  for (const entry of state?.stack_objects || []) {
    if (String(entry?.id) === needle) return true;
  }

  return false;
}

function locateObjectInState(state, objectId) {
  if (objectId == null) return null;
  const needle = String(objectId);
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

  for (const entry of state?.stack_objects || []) {
    if (String(entry?.id) === needle || String(entry?.inspect_object_id) === needle) {
      return { side: "stack", zone: "stack" };
    }
  }

  return null;
}

function shouldUseTopInlineDock(location) {
  return (
    location?.side === "self"
    && location?.zone !== "stack"
  );
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

export default function RightRail({
  pinnedObjectId,
  inspectorBottomOffset = DEFAULT_INSPECTOR_BOTTOM_OFFSET,
  inline = false,
  inlineExpanded = false,
  inlineDockPlacement = "bottom",
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
  const hoveredObjectId = useHoveredObjectId();
  const decision = state?.decision || null;
  const stackObjects = state?.stack_objects || [];
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
  const relevantPinnedObjectId = focusedDecision && pinnedInspectorObjectId != null
    ? (decisionReferencesObject(decision, pinnedInspectorObjectId) ? pinnedInspectorObjectId : null)
    : pinnedInspectorObjectId;

  // During non-priority decision steps (targeting, choose number/options, etc),
  // keep inspector focus on the spell being cast/resolved instead of hover.
  const decisionLockedObjectId = focusedDecision
    ? (resolvingCastObjectId ?? relevantPinnedObjectId ?? hoveredObjectId ?? topStackObjectId)
    : null;

  const selectedObjectId = focusedDecision
    ? decisionLockedObjectId
    : (relevantPinnedObjectId ?? hoveredObjectId ?? resolvingCastObjectId ?? topStackObjectId);
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
  const prefersTopInlineDock = (
    inline
    && allowTopInlinePlacement
    && shouldUseTopInlineDock(selectedObjectLocation)
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
      prefersTopInlineDock
        ? inlineDockPlacement === "top"
        : inlineDockPlacement === "bottom"
    )
  );
  const shouldRenderExpandedInlineInspector =
    inline
    && shouldShowRail
    && (inlineExpanded || hoveredObjectId != null);
  const useExpandedInlineInspector =
    shouldRenderExpandedInlineInspector
    && (
      forceInlineExpanded
      || hoveredObjectId != null
      || (
        hoveredObjectId != null
        && pinnedInspectorObjectId != null
        && validSelectedObjectId != null
        && String(hoveredObjectId) === String(pinnedInspectorObjectId)
        && String(validSelectedObjectId) === String(pinnedInspectorObjectId)
      )
    );
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
      shouldShowRail,
      useExpandedInlineInspector,
    ]
  );
  const expandedInlineShellOffset = inlineExpandedSide === "left"
    ? { left: `-${INLINE_EXPANDED_RIGHT_BLEED}px`, right: "auto", transformOrigin: "bottom left" }
    : { left: "auto", right: `-${INLINE_EXPANDED_RIGHT_BLEED}px`, transformOrigin: "bottom right" };

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
        >
          <div className="relative h-full min-h-0 overflow-hidden">
            <HoverArtOverlay
              objectId={shouldShowRail ? validSelectedObjectId : null}
              suppressStableId={inspectorSuppressStableId}
              compact={inline}
              onPreferredWidthChange={inline ? setPreferredInlineWidth : null}
            />
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
            }}
          >
            <HoverArtOverlay
              objectId={shouldShowRail ? validSelectedObjectId : null}
              suppressStableId={inspectorSuppressStableId}
              displayMode={fullArtInlineExpanded ? "full-art" : "inspector"}
              availableInspectorWidth={expandedInlineWidth}
              availableInspectorHeight={expandedInlineHeight}
              onPreferredInspectorWidthChange={
                fullArtInlineExpanded ? null : setPreferredExpandedInlineWidth
              }
            />
          </div>
        )}
      </div>
    </aside>
  );
}
