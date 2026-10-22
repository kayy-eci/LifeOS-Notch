import { useEffect, useState } from "react";
import { Plus, Trash2, Droplets } from "lucide-react";
import { useWaterStore } from "../../stores/useWaterStore";
import { useSettingsStore } from "../../stores/useSettingsStore";
import { formatVolume } from "../../utils/format";
import { Button } from "../../components/ui/Button";
import { IconButton } from "../../components/ui/IconButton";
import { ProgressBar } from "../../components/ui/ProgressBar";
import { ErrorNotice } from "../../components/ui/ErrorNotice";
import { ConfirmDialog } from "../../components/ui/ConfirmDialog";

/**
 * Water intake.
 *
 * Laid out as a dense log rather than a card: the useful information is the
 * remaining amount and the individual entries, and both are lists.
 */
export function WaterPanel() {
  const day = useWaterStore((s) => s.day);
  const error = useWaterStore((s) => s.error);
  const load = useWaterStore((s) => s.load);
  const add = useWaterStore((s) => s.add);
  const remove = useWaterStore((s) => s.remove);
  const clear = useWaterStore((s) => s.clear);

  const settings = useSettingsStore((s) => s.settings);
  const [confirmClear, setConfirmClear] = useState(false);

  useEffect(() => {
    if (!day) void load();
  }, [day, load]);

  if (!day) {
    return <p className="py-8 text-center text-xs text-text-subtle">Loading today's intake…</p>;
  }

  const serving = settings?.water_serving_ml ?? 250;
  const remaining = Math.max(0, day.target_ml - day.total_ml);
  const pct = day.target_ml > 0 ? day.total_ml / day.target_ml : 0;

  return (
    <div className="flex flex-col gap-3">
      {error ? <ErrorNotice message={error} className="w-full" onRetry={() => void load()} /> : null}

      <div>
        <div className="mb-1 flex items-baseline justify-between">
          <p className="text-sm text-text">
            <span className="tabular text-lg font-medium">{formatVolume(day.total_ml)}</span>
            <span className="text-text-subtle"> of {formatVolume(day.target_ml)}</span>
          </p>
          <span className="tabular text-2xs text-text-subtle">{Math.round(pct * 100)}%</span>
        </div>
        <ProgressBar value={pct} label="Daily water progress" />
        <p className="mt-1 text-2xs text-text-subtle">
          {day.goal_reached ? "Target reached for today." : `${formatVolume(remaining)} to go`}
        </p>
      </div>

      <div className="flex flex-wrap items-center gap-1.5">
        <Button size="sm" variant="primary" onClick={() => void add(serving)}>
          <Plus size={12} aria-hidden />
          {formatVolume(serving)}
        </Button>
        <Button size="sm" onClick={() => void add(Math.round(serving / 2))}>
          {formatVolume(Math.round(serving / 2))}
        </Button>
        <Button size="sm" onClick={() => void add(serving * 2)}>
          {formatVolume(serving * 2)}
        </Button>
        <Button size="sm" variant="ghost" onClick={() => setConfirmClear(true)} disabled={day.entries.length === 0}>
          <Trash2 size={12} aria-hidden />
          Clear day
        </Button>
      </div>

      <div className="border-t border-border pt-2">
        <p className="mb-1 text-2xs text-text-subtle">
          {day.entries.length === 0 ? "No intake logged yet." : `${day.entries.length} logged today`}
        </p>

        {day.entries.length === 0 ? (
          <p className="py-4 text-center text-xs text-text-subtle">
            Log your first glass with the buttons above.
          </p>
        ) : (
          <ul className="flex flex-col">
            {day.entries
              .slice()
              .reverse()
              .map((entry) => (
                <li
                  key={entry.id}
                  className="group flex items-center gap-2 rounded-control px-1 py-1 hover:bg-surface-raised"
                >
                  <Droplets size={11} className="shrink-0 text-accent" aria-hidden />
                  <span className="tabular text-xs text-text">{formatVolume(entry.amount_ml)}</span>
                  <span className="tabular ml-auto text-2xs text-text-subtle">
                    {new Date(entry.logged_at).toLocaleTimeString(undefined, {
                      hour: "2-digit",
                      minute: "2-digit",
                    })}
                  </span>
                  <IconButton
                    label={`Remove the ${formatVolume(entry.amount_ml)} entry`}
                    size="sm"
                    onClick={() => void remove(entry.id)}
                    className="opacity-0 group-hover:opacity-100 focus-visible:opacity-100"
                  >
                    <Trash2 size={11} aria-hidden />
                  </IconButton>
                </li>
              ))}
          </ul>
        )}
      </div>

      <ConfirmDialog
        open={confirmClear}
        title="Clear today's intake?"
        message={`This removes all ${day.entries.length} entries logged today. It cannot be undone.`}
        confirmLabel="Clear"
        destructive
        onConfirm={() => {
          void clear();
          setConfirmClear(false);
        }}
        onCancel={() => setConfirmClear(false)}
      />
    </div>
  );
}
