import { useEffect, useState } from "react";
import { Check, Plus, Trash2, RefreshCw, CloudOff, CircleAlert } from "lucide-react";
import { useTasksStore } from "../../stores/useTasksStore";
import { useNotionStore } from "../../stores/useNotionStore";
import { formatDueDate } from "../../utils/format";
import { Button } from "../../components/ui/Button";
import { IconButton } from "../../components/ui/IconButton";
import { Spinner } from "../../components/ui/Spinner";
import { ErrorNotice } from "../../components/ui/ErrorNotice";

/** How each sync state reads to the user. */
const SYNC_LABEL: Record<string, string> = {
  local: "Not synced",
  pending: "Waiting to sync",
  synced: "Synced",
  conflict: "Changed in both places",
  failed: "Sync failed",
};

/**
 * The task list.
 *
 * A compact list, not a card grid: the only actions are add, complete, rename
 * and delete, and all four work on one row at a time.
 */
export function TodosPanel() {
  const tasks = useTasksStore((s) => s.tasks);
  const loading = useTasksStore((s) => s.loading);
  const error = useTasksStore((s) => s.error);
  const load = useTasksStore((s) => s.load);
  const create = useTasksStore((s) => s.create);
  const toggle = useTasksStore((s) => s.toggle);
  const remove = useTasksStore((s) => s.remove);
  const rename = useTasksStore((s) => s.rename);

  const notion = useNotionStore((s) => s.status);
  const sync = useNotionStore((s) => s.sync);
  const syncing = useNotionStore((s) => s.syncing);

  const [draft, setDraft] = useState("");
  const [editingId, setEditingId] = useState<string | null>(null);
  const [editingValue, setEditingValue] = useState("");

  useEffect(() => {
    if (tasks.length === 0 && !loading) void load();
  }, [tasks.length, loading, load]);

  const connected = notion?.state === "connected";
  const open = tasks.filter((t) => !t.done);
  const done = tasks.filter((t) => t.done);

  const submit = () => {
    const value = draft.trim();
    if (!value) return;
    void create(value);
    setDraft("");
  };

  return (
    <div className="flex flex-col gap-2">
      {/* Notion state: only shown when it is actionable or unusual. */}
      {!connected && notion?.configured ? (
        <p className="flex items-center gap-1.5 rounded-control bg-surface-raised px-2 py-1.5 text-2xs text-text-subtle">
          <CloudOff size={11} aria-hidden />
          Working offline. Tasks stay here and sync when you reconnect Notion in Settings.
        </p>
      ) : null}

      {connected ? (
        <div className="flex items-center gap-2 text-2xs text-text-subtle">
          {syncing ? (
            <Spinner label="Syncing" />
          ) : (
            <RefreshCw size={11} aria-hidden />
          )}
          <span>
            {syncing
              ? "Syncing with Notion…"
              : notion?.pending_changes
                ? `${notion.pending_changes} change${notion.pending_changes === 1 ? "" : "s"} waiting to sync`
                : "Up to date with Notion"}
          </span>
          <button
            type="button"
            onClick={() => void sync()}
            disabled={syncing}
            className="ml-auto rounded-control px-1.5 py-0.5 hover:bg-surface-hover hover:text-text disabled:opacity-50"
          >
            Sync now
          </button>
        </div>
      ) : null}

      {error ? <ErrorNotice message={error} onRetry={() => void load()} /> : null}

      <form
        className="flex items-center gap-1.5"
        onSubmit={(e) => {
          e.preventDefault();
          submit();
        }}
      >
        <input
          value={draft}
          onChange={(e) => setDraft(e.target.value)}
          placeholder="Add a task"
          aria-label="New task title"
          maxLength={500}
          className="h-8 min-w-0 flex-1 rounded-control border border-border bg-surface-sunken px-2
            text-sm text-text placeholder:text-text-subtle hover:border-border-strong
            focus:border-accent focus:outline-none"
        />
        <Button size="sm" variant="primary" type="submit" disabled={!draft.trim()}>
          <Plus size={12} aria-hidden />
          Add
        </Button>
      </form>

      {loading && tasks.length === 0 ? (
        <p className="py-6 text-center text-xs text-text-subtle">Loading tasks…</p>
      ) : tasks.length === 0 ? (
        <p className="py-6 text-center text-xs text-text-subtle">
          No tasks yet. Add one above, or connect Notion in Settings to bring an existing list in.
        </p>
      ) : (
        <>
          <ul className="flex flex-col">
            {open.map((task) => (
              <li
                key={task.id}
                className="group flex items-start gap-2 rounded-control px-1 py-1.5 hover:bg-surface-raised"
              >
                <button
                  type="button"
                  aria-label={`Mark "${task.title}" complete`}
                  onClick={() => void toggle(task)}
                  className="mt-0.5 grid size-4 shrink-0 place-items-center rounded-[3px] border border-border-strong
                    text-transparent hover:border-accent hover:text-accent"
                >
                  <Check size={11} aria-hidden />
                </button>

                <div className="min-w-0 flex-1">
                  {editingId === task.id ? (
                    <input
                      autoFocus
                      value={editingValue}
                      onChange={(e) => setEditingValue(e.target.value)}
                      onBlur={() => {
                        void rename(task, editingValue);
                        setEditingId(null);
                      }}
                      onKeyDown={(e) => {
                        if (e.key === "Enter") {
                          void rename(task, editingValue);
                          setEditingId(null);
                        }
                        if (e.key === "Escape") setEditingId(null);
                      }}
                      className="h-6 w-full rounded-[3px] border border-accent bg-surface-sunken px-1 text-sm text-text focus:outline-none"
                    />
                  ) : (
                    <button
                      type="button"
                      onClick={() => {
                        setEditingId(task.id);
                        setEditingValue(task.title);
                      }}
                      className="block w-full truncate text-left text-sm text-text"
                      title={task.title}
                    >
                      {task.title}
                    </button>
                  )}

                  <div className="mt-0.5 flex items-center gap-2 text-2xs text-text-subtle">
                    {task.due_at ? <span>{formatDueDate(task.due_at)}</span> : null}
                    {task.sync_state !== "synced" ? (
                      <span
                        className={
                          task.sync_state === "conflict" || task.sync_state === "failed"
                            ? "flex items-center gap-1 text-warning"
                            : "flex items-center gap-1"
                        }
                      >
                        {task.sync_state === "conflict" || task.sync_state === "failed" ? (
                          <CircleAlert size={9} aria-hidden />
                        ) : null}
                        {SYNC_LABEL[task.sync_state] ?? task.sync_state}
                      </span>
                    ) : null}
                  </div>
                </div>

                <IconButton
                  label={`Delete "${task.title}"`}
                  size="sm"
                  onClick={() => void remove(task)}
                  className="opacity-0 group-hover:opacity-100 focus-visible:opacity-100"
                >
                  <Trash2 size={11} aria-hidden />
                </IconButton>
              </li>
            ))}
          </ul>

          {done.length > 0 ? (
            <div className="border-t border-border pt-1.5">
              <p className="mb-1 text-2xs text-text-subtle">{done.length} completed</p>
              <ul className="flex flex-col">
                {done.map((task) => (
                  <li key={task.id} className="group flex items-center gap-2 rounded-control px-1 py-1">
                    <button
                      type="button"
                      aria-label={`Reopen "${task.title}"`}
                      onClick={() => void toggle(task)}
                      className="grid size-4 shrink-0 place-items-center rounded-[3px] border border-accent bg-accent text-accent-contrast"
                    >
                      <Check size={11} aria-hidden />
                    </button>
                    <span className="min-w-0 flex-1 truncate text-xs text-text-subtle line-through">
                      {task.title}
                    </span>
                    <IconButton
                      label={`Delete "${task.title}"`}
                      size="sm"
                      onClick={() => void remove(task)}
                      className="opacity-0 group-hover:opacity-100 focus-visible:opacity-100"
                    >
                      <Trash2 size={11} aria-hidden />
                    </IconButton>
                  </li>
                ))}
              </ul>
            </div>
          ) : null}
        </>
      )}
    </div>
  );
}
