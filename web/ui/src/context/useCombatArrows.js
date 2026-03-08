import { useContext } from "react";
import { CombatArrowContext } from "@/context/CombatArrowContext.shared";

export function useCombatArrows() {
  const ctx = useContext(CombatArrowContext);
  if (!ctx) throw new Error("useCombatArrows must be inside CombatArrowProvider");
  return ctx;
}
