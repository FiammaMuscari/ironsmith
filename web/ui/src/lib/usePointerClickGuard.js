import { useCallback, useRef } from "react";

const DEFAULT_SUPPRESS_MS = 450;

// Prevent pointerdown-triggered actions from being re-fired by the click
// event that browsers dispatch immediately afterwards.
export function usePointerClickGuard(suppressMs = DEFAULT_SUPPRESS_MS) {
  const lastPointerDownTsRef = useRef(-Infinity);

  const registerPointerDown = useCallback((event) => {
    if (event?.button != null && event.button !== 0) return false;
    const timeStamp = Number(event?.timeStamp);
    lastPointerDownTsRef.current = Number.isFinite(timeStamp)
      ? timeStamp
      : performance.now();
    return true;
  }, []);

  const shouldHandleClick = useCallback((event) => {
    const timeStamp = Number(event?.timeStamp);
    const clickTime = Number.isFinite(timeStamp)
      ? timeStamp
      : performance.now();
    const shouldSuppress = (clickTime - lastPointerDownTsRef.current) < suppressMs;
    if (shouldSuppress) {
      lastPointerDownTsRef.current = -Infinity;
      return false;
    }
    return true;
  }, [suppressMs]);

  return {
    registerPointerDown,
    shouldHandleClick,
  };
}
