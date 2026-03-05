import { useCallback, useEffect, useState } from "react";
import { useGame } from "@/context/GameContext";
import { Button } from "@/components/ui/button";
import { ScrollArea } from "@/components/ui/scroll-area";
import { normalizeDecisionText } from "./decisionText";
import { Input } from "@/components/ui/input";

export default function NumberDecision({
  decision,
  canAct,
  inlineSubmit = true,
  onSubmitActionChange = null,
  hideDescription = false,
}) {
  const { dispatch } = useGame();
  const [value, setValue] = useState(decision.min ?? 0);
  const handleSubmit = useCallback(() => {
    dispatch({ type: "number_choice", value }, `Chose ${value}`);
  }, [dispatch, value]);

  useEffect(() => {
    if (!onSubmitActionChange) return undefined;
    onSubmitActionChange({
      label: "Submit",
      disabled: !canAct,
      onSubmit: handleSubmit,
    });
    return () => onSubmitActionChange(null);
  }, [onSubmitActionChange, canAct, handleSubmit]);

  return (
    <div className="flex h-full min-h-0 flex-col gap-2">
      <ScrollArea className="flex-1 min-h-0">
        <div className="flex flex-col gap-2 pr-1">
          {!hideDescription && decision.description && (
            <div className="text-[16px] text-muted-foreground">{normalizeDecisionText(decision.description)}</div>
          )}
          {decision.source_name && (
            <div className="text-[14px] italic text-[#c0a060]">{decision.source_name}</div>
          )}
          <div className="flex items-center gap-2">
            <Input
              type="number"
              className="h-7 w-28 text-[16px] bg-transparent"
              min={decision.min ?? 0}
              max={decision.max ?? 999}
              value={value}
              onChange={(e) => setValue(Number(e.target.value))}
              disabled={!canAct}
            />
            <span className="text-[14px] text-muted-foreground">
              ({decision.min} - {decision.max})
            </span>
          </div>
        </div>
      </ScrollArea>
      {inlineSubmit && (
        <div className="shrink-0 border-t border-game-line-2/70 pt-1">
          <Button
            variant="ghost"
            size="sm"
            className="h-7 text-[14px] text-muted-foreground transition-all hover:text-foreground hover:bg-[rgba(100,169,255,0.1)] hover:shadow-[0_0_8px_rgba(100,169,255,0.15)]"
            disabled={!canAct}
            onClick={handleSubmit}
          >
            Submit
          </Button>
        </div>
      )}
    </div>
  );
}
