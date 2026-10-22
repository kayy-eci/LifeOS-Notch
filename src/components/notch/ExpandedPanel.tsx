import { Music2, Timer, Droplets, ListTodo, Settings } from "lucide-react";
import type { PanelId } from "../../stores/useNotchStore";
import { useNotchStore } from "../../stores/useNotchStore";
import { MusicPanel } from "../../features/music/MusicPanel";
import { PomodoroPanel } from "../../features/pomodoro/PomodoroPanel";
import { WaterPanel } from "../../features/water/WaterPanel";
import { TodosPanel } from "../../features/todos/TodosPanel";
import { SettingsPanel } from "../../features/settings/SettingsPanel";
import { IconButton } from "../ui/IconButton";

const NAV: { id: PanelId; label: string; icon: typeof Music2 }[] = [
  { id: "music", label: "Music", icon: Music2 },
  { id: "pomodoro", label: "Focus", icon: Timer },
  { id: "water", label: "Water", icon: Droplets },
  { id: "todos", label: "Tasks", icon: ListTodo },
  { id: "settings", label: "Settings", icon: Settings },
];

/**
 * The expanded panel.
 *
 * Navigation is a single row of icons rather than a nested menu: the panel is
 * one click deep from the desktop, and anything nested costs more than it
 * saves.
 */
export function ExpandedPanel({
  activePanel,
  dateLabel,
  onTogglePin,
}: {
  activePanel: PanelId;
  dateLabel: string;
  onTogglePin: () => void;
}) {
  const setActivePanel = useNotchStore((s) => s.setActivePanel);

  return (
    <div className="flex min-h-0 flex-1 animate-slide-up flex-col rounded-b-notch bg-surface">
      <div className="flex shrink-0 items-center gap-1 border-b border-border px-2 py-1.5">
        <p className="mr-auto text-2xs text-text-subtle">{dateLabel}</p>
        {NAV.map(({ id, label, icon: Icon }) => (
          <IconButton
            key={id}
            label={label}
            size="sm"
            active={activePanel === id}
            onClick={() => setActivePanel(id)}
          >
            <Icon size={14} aria-hidden />
          </IconButton>
        ))}
        <button
          type="button"
          onClick={onTogglePin}
          className="ml-1 rounded-control px-1.5 py-0.5 text-2xs text-text-subtle hover:bg-surface-hover hover:text-text"
        >
          Close
        </button>
      </div>

      <div className="scroll-thin min-h-0 flex-1 overflow-y-auto p-3">
        {activePanel === "music" ? <MusicPanel showLyrics /> : null}
        {activePanel === "pomodoro" ? <PomodoroPanel /> : null}
        {activePanel === "water" ? <WaterPanel /> : null}
        {activePanel === "todos" ? <TodosPanel /> : null}
        {activePanel === "settings" ? <SettingsPanel /> : null}
      </div>
    </div>
  );
}
