import { useCallback, useEffect, useRef, useState } from "react";

const DEFAULT_IDLE_MS = 140;

export function useHoverSuppressedWhileScrolling({ onScrollStart, idleMs = DEFAULT_IDLE_MS } = {}) {
  const cleanupRef = useRef(null);
  const timeoutRef = useRef(null);
  const onScrollStartRef = useRef(onScrollStart);
  const [hoverSuppressed, setHoverSuppressed] = useState(false);

  useEffect(() => {
    onScrollStartRef.current = onScrollStart;
  }, [onScrollStart]);

  const markScrollingActive = useCallback(() => {
    setHoverSuppressed((wasSuppressed) => {
      if (!wasSuppressed) {
        onScrollStartRef.current?.();
      }
      return true;
    });

    if (timeoutRef.current) {
      clearTimeout(timeoutRef.current);
    }
    timeoutRef.current = window.setTimeout(() => {
      timeoutRef.current = null;
      setHoverSuppressed(false);
    }, idleMs);
  }, [idleMs]);

  const attachScrollableRef = useCallback((node) => {
    cleanupRef.current?.();
    cleanupRef.current = null;

    if (!node) return;

    node.addEventListener("scroll", markScrollingActive, { passive: true });
    node.addEventListener("wheel", markScrollingActive, { passive: true });
    node.addEventListener("touchmove", markScrollingActive, { passive: true });

    cleanupRef.current = () => {
      node.removeEventListener("scroll", markScrollingActive);
      node.removeEventListener("wheel", markScrollingActive);
      node.removeEventListener("touchmove", markScrollingActive);
    };
  }, [markScrollingActive]);

  useEffect(
    () => () => {
      cleanupRef.current?.();
      cleanupRef.current = null;
      if (timeoutRef.current) {
        clearTimeout(timeoutRef.current);
        timeoutRef.current = null;
      }
    },
    []
  );

  return {
    attachScrollableRef,
    hoverSuppressed,
  };
}
