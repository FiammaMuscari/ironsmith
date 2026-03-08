import { useEffect, useLayoutEffect, useRef } from "react";
import { animate, cancelMotion } from "@/lib/motion/anime";
import { cn } from "@/lib/utils";

function glowPhaseFromSeed(seed) {
  let hash = 0;
  const text = String(seed || "");
  for (let i = 0; i < text.length; i++) {
    hash = ((hash * 31) + text.charCodeAt(i)) | 0;
  }
  return Math.abs(hash);
}

export default function AnimatedCircuitFrame({
  seed = "",
  path = "",
  viewBox = "0 0 100 140",
  className = "",
  overlayClassName = "",
  svgClassName = "",
}) {
  const glowRef = useRef(null);
  const coreRef = useRef(null);
  const accentRef = useRef(null);
  const motionRefs = useRef([]);
  const glowPhase = glowPhaseFromSeed(seed);

  useLayoutEffect(() => {
    motionRefs.current.forEach(cancelMotion);
    motionRefs.current = [];

    if (!path) return undefined;

    const startOffset = -((glowPhase % 1000) + 100);
    const primaryNodes = [glowRef.current, coreRef.current].filter(Boolean);

    if (primaryNodes.length > 0) {
      motionRefs.current.push(
        animate(primaryNodes, {
          strokeDashoffset: [startOffset, startOffset - 1000],
          ease: "linear",
          duration: 2400 + (glowPhase % 900),
          loop: true,
        })
      );
    }

    if (accentRef.current) {
      motionRefs.current.push(
        animate(accentRef.current, {
          strokeDashoffset: [startOffset - 460, startOffset - 1460],
          ease: "linear",
          duration: 4200 + (glowPhase % 1400),
          loop: true,
        })
      );
    }

    return () => {
      motionRefs.current.forEach(cancelMotion);
      motionRefs.current = [];
    };
  }, [glowPhase, path]);

  useEffect(() => () => {
    motionRefs.current.forEach(cancelMotion);
    motionRefs.current = [];
  }, []);

  if (!path) return null;

  return (
    <div className={cn("card-circuit-overlay", overlayClassName)} aria-hidden="true">
      <svg
        className={cn("card-circuit-svg", svgClassName, className)}
        viewBox={viewBox}
        preserveAspectRatio="none"
      >
        <path className="card-circuit-track" d={path} pathLength="1000" />
        <path ref={glowRef} className="card-circuit-glow" d={path} pathLength="1000" />
        <path ref={coreRef} className="card-circuit-core" d={path} pathLength="1000" />
        <path ref={accentRef} className="card-circuit-accent" d={path} pathLength="1000" />
      </svg>
    </div>
  );
}
