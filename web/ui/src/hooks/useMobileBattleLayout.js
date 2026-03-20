import { useEffect, useMemo, useState } from "react";
import { solveMobileBattleLayout } from "@/lib/mobile-battle-layout";

function readVisualViewport() {
  if (typeof window === "undefined") {
    return { width: 844, height: 390 };
  }
  const vv = window.visualViewport;
  return {
    width: Math.floor(vv?.width || window.innerWidth || 844),
    height: Math.floor(vv?.height || window.innerHeight || 390),
  };
}

function readSafeAreaInset(name) {
  if (typeof document === "undefined") return 0;
  const raw = getComputedStyle(document.documentElement).getPropertyValue(name);
  const parsed = Number.parseFloat(raw || "0");
  return Number.isFinite(parsed) ? parsed : 0;
}

export default function useMobileBattleLayout(config) {
  const [viewport, setViewport] = useState(() => readVisualViewport());

  useEffect(() => {
    if (typeof window === "undefined") return undefined;
    const updateViewport = () => {
      setViewport((current) => {
        const next = readVisualViewport();
        return current.width === next.width && current.height === next.height
          ? current
          : next;
      });
    };

    updateViewport();
    window.addEventListener("resize", updateViewport);
    window.visualViewport?.addEventListener("resize", updateViewport);
    window.visualViewport?.addEventListener("scroll", updateViewport);

    return () => {
      window.removeEventListener("resize", updateViewport);
      window.visualViewport?.removeEventListener("resize", updateViewport);
      window.visualViewport?.removeEventListener("scroll", updateViewport);
    };
  }, []);

  return useMemo(
    () => solveMobileBattleLayout({
      ...config,
      viewportWidth: viewport.width,
      viewportHeight: viewport.height,
      safeAreaTop: readSafeAreaInset("--sat") || 0,
      safeAreaBottom: readSafeAreaInset("--sab") || 0,
    }),
    [config, viewport.height, viewport.width]
  );
}
