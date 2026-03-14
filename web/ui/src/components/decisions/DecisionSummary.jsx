import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { useGame } from "@/context/GameContext";
import { SymbolText } from "@/lib/mana-symbols";
import { getVisibleTopStackObject } from "@/lib/stack-targets";
import { cn } from "@/lib/utils";
import { normalizeDecisionText } from "./decisionText";

function normalizeLine(text) {
  if (typeof text !== "string") return "";
  return normalizeDecisionText(text).trim();
}

function sameLine(left, right) {
  return normalizeLine(left).toLowerCase() === normalizeLine(right).toLowerCase();
}

function AutoFitStripLine({
  text,
  className,
  baseSize,
  minSize,
}) {
  const containerRef = useRef(null);
  const contentRef = useRef(null);
  const [fontSize, setFontSize] = useState(baseSize);

  useEffect(() => {
    setFontSize(baseSize);
  }, [baseSize, text]);

  useLayoutEffect(() => {
    const container = containerRef.current;
    const content = contentRef.current;
    if (!container || !content) return undefined;

    let frame = null;

    const measureAt = (size) => {
      content.style.fontSize = `${size}px`;
      return content.scrollWidth <= container.clientWidth + 1;
    };

    const fit = () => {
      frame = requestAnimationFrame(() => {
        if (!containerRef.current || !contentRef.current) return;
        if (containerRef.current.clientWidth <= 0) return;

        let low = minSize;
        let high = baseSize;
        let best = minSize;

        while (low <= high) {
          const mid = Math.floor((low + high) / 2);
          if (measureAt(mid)) {
            best = mid;
            low = mid + 1;
          } else {
            high = mid - 1;
          }
        }

        contentRef.current.style.fontSize = `${best}px`;
        setFontSize(best);
      });
    };

    fit();

    const resizeObserver = new ResizeObserver(fit);
    resizeObserver.observe(container);

    return () => {
      resizeObserver.disconnect();
      if (frame != null) cancelAnimationFrame(frame);
    };
  }, [baseSize, minSize, text]);

  return (
    <div ref={containerRef} className="min-w-0 overflow-hidden">
      <div
        ref={contentRef}
        className={cn("overflow-hidden leading-tight whitespace-nowrap", className)}
      >
        <SymbolText
          text={text}
          noWrap
          symbolSize={Math.max(10, Math.round(fontSize + 1))}
        />
      </div>
    </div>
  );
}

export default function DecisionSummary({
  decision,
  hideDescription = false,
  layout = "panel",
  className = "",
}) {
  const { state } = useGame();
  if (!decision) return null;
  if (layout === "strip" && hideDescription) return null;

  const stripLayout = layout === "strip";
  const description = hideDescription ? "" : normalizeLine(decision.description);
  const topStackObject = getVisibleTopStackObject(state);
  const resolvingStackContextText = (() => {
    if (!topStackObject?.ability_kind) return "";
    if (decision.source_id == null) return "";
    const sourceId = String(decision.source_id);
    const stackSourceId = topStackObject.inspect_object_id != null
      ? String(topStackObject.inspect_object_id)
      : (topStackObject.id != null ? String(topStackObject.id) : "");
    if (!stackSourceId || stackSourceId !== sourceId) return "";
    return normalizeLine(topStackObject.ability_text || topStackObject.effect_text || "");
  })();
  const contextText = resolvingStackContextText || normalizeLine(decision.context_text);
  const consequenceText = normalizeLine(decision.consequence_text);

  const lines = [];
  if (stripLayout) {
    if (description) {
      lines.push({
        key: "description",
        text: description,
        className: "text-[#c7dcf3]",
        baseSize: 13,
        minSize: 9,
      });
    }
    const secondarySegments = [];
    if (contextText && !sameLine(contextText, description)) {
      secondarySegments.push(contextText);
    }
    if (consequenceText && !sameLine(consequenceText, description) && !sameLine(consequenceText, contextText)) {
      secondarySegments.push(`Follow-up: ${consequenceText}`);
    }
    if (secondarySegments.length > 0) {
      lines.push({
        key: "secondary",
        text: secondarySegments.join(" | "),
        className: "text-[#8fb5d8]",
        baseSize: 11,
        minSize: 9,
      });
    }
  } else {
    if (description) {
      lines.push({
        key: "description",
        text: description,
        className: "text-[14px] text-[#c7dcf3]",
      });
    }
    if (contextText && !sameLine(contextText, description)) {
      lines.push({
        key: "context",
        text: contextText,
        className: "text-[13px] text-[#8fb5d8]",
      });
    }
    if (consequenceText && !sameLine(consequenceText, description) && !sameLine(consequenceText, contextText)) {
      lines.push({
        key: "consequence",
        text: `Follow-up: ${consequenceText}`,
        className: "text-[13px] text-[#f0cf8a]",
      });
    }
  }

  if (lines.length === 0) return null;

  return (
    <div
      className={cn(
        stripLayout
          ? "grid h-[30px] min-w-0 grid-rows-2 gap-y-0.5 px-1"
          : "flex flex-col gap-0.5 px-1 leading-snug",
        className
      )}
    >
      {stripLayout ? (
        <>
          {lines.map((line) => (
            <AutoFitStripLine
              key={line.key}
              text={line.text}
              className={line.className}
              baseSize={line.baseSize}
              minSize={line.minSize}
            />
          ))}
          {lines.length < 2 && <div aria-hidden="true" className="min-w-0" />}
        </>
      ) : (
        lines.map((line) => (
          <div key={line.key} className={line.className}>
            <SymbolText text={line.text} />
          </div>
        ))
      )}
    </div>
  );
}
