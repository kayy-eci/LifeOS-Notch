import { useEffect, useState } from "react";
import { Plug, PlugZap, Unplug } from "lucide-react";
import { useSettingsStore } from "../../stores/useSettingsStore";
import { useNotionStore } from "../../stores/useNotionStore";
import { Button } from "../../components/ui/Button";
import { Toggle, TextInput, Select } from "../../components/ui/Field";
import { ErrorNotice } from "../../components/ui/ErrorNotice";
import { Spinner } from "../../components/ui/Spinner";

/**
 * Settings.
 *
 * Grouped by feature rather than by control type, so a user changing their
 * water goal does not have to scroll past timer settings to find it.
 */
export function SettingsPanel() {
  const settings = useSettingsStore((s) => s.settings);
  const loading = useSettingsStore((s) => s.loading);
  const error = useSettingsStore((s) => s.error);
  const load = useSettingsStore((s) => s.load);
  const update = useSettingsStore((s) => s.update);

  const notion = useNotionStore((s) => s.status);
  const notionError = useNotionStore((s) => s.error);
  const connecting = useNotionStore((s) => s.connecting);
  const connect = useNotionStore((s) => s.connect);
  const disconnect = useNotionStore((s) => s.disconnect);
  const loadNotion = useNotionStore((s) => s.load);
  const databases = useNotionStore((s) => s.databases);
  const loadDatabases = useNotionStore((s) => s.loadDatabases);
  const selectDatabase = useNotionStore((s) => s.selectDatabase);
  const loadingDatabases = useNotionStore((s) => s.loadingDatabases);

  const [clientId, setClientId] = useState("");
  const [clientSecret, setClientSecret] = useState("");
  const configure = useNotionStore((s) => s.configure);

  useEffect(() => {
    if (!settings) void load();
    if (!notion) void loadNotion();
  }, [settings, notion, load, loadNotion]);

  if (loading && !settings) {
    return <p className="py-8 text-center text-xs text-text-subtle">Loading settings…</p>;
  }
  if (!settings) {
    return <ErrorNotice message="Settings could not be loaded." onRetry={() => void load()} />;
  }

  const set = (patch: Parameters<typeof update>[0]) => void update(patch);

  return (
    <div className="flex flex-col gap-4">
      {error ? <ErrorNotice message={error} onRetry={() => void load()} /> : null}

      {/* Focus */}
      <Section title="Focus timer">
        <div className="grid grid-cols-3 gap-2">
          <NumberField
            label="Focus"
            value={settings.focus_minutes}
            min={1}
            max={180}
            suffix="min"
            onChange={(focus_minutes) => set({ focus_minutes })}
          />
          <NumberField
            label="Short break"
            value={settings.short_break_minutes}
            min={1}
            max={60}
            suffix="min"
            onChange={(short_break_minutes) => set({ short_break_minutes })}
          />
          <NumberField
            label="Long break"
            value={settings.long_break_minutes}
            min={1}
            max={120}
            suffix="min"
            onChange={(long_break_minutes) => set({ long_break_minutes })}
          />
        </div>
        <Toggle
          label="Start breaks automatically"
          hint="A break begins as soon as a focus session ends."
          checked={settings.auto_start_breaks}
          onChange={(auto_start_breaks) => set({ auto_start_breaks })}
        />
        <Toggle
          label="Start the next focus session automatically"
          hint="Off by default, so returning to work is a deliberate choice."
          checked={settings.auto_start_focus}
          onChange={(auto_start_focus) => set({ auto_start_focus })}
        />
      </Section>

      {/* Water */}
      <Section title="Water">
        <div className="grid grid-cols-2 gap-2">
          <NumberField
            label="Daily target"
            value={settings.water_target_ml}
            min={250}
            max={10000}
            step={250}
            suffix="ml"
            onChange={(water_target_ml) => set({ water_target_ml })}
          />
          <NumberField
            label="Serving size"
            value={settings.water_serving_ml}
            min={10}
            max={2000}
            step={50}
            suffix="ml"
            onChange={(water_serving_ml) => set({ water_serving_ml })}
          />
        </div>
        <p className="text-2xs text-text-subtle">
          This is your own target, not a medical recommendation.
        </p>
        <Toggle
          label="Remind me to drink"
          checked={settings.water_reminder_enabled}
          onChange={(water_reminder_enabled) => set({ water_reminder_enabled })}
        />
        {settings.water_reminder_enabled ? (
          <div className="grid grid-cols-3 gap-2">
            <NumberField
              label="Every"
              value={settings.water_reminder_interval_minutes}
              min={5}
              max={480}
              step={5}
              suffix="min"
              onChange={(water_reminder_interval_minutes) => set({ water_reminder_interval_minutes })}
            />
            <NumberField
              label="From"
              value={settings.water_reminder_start_hour}
              min={0}
              max={23}
              suffix="h"
              onChange={(water_reminder_start_hour) => set({ water_reminder_start_hour })}
            />
            <NumberField
              label="Until"
              value={settings.water_reminder_end_hour}
              min={0}
              max={23}
              suffix="h"
              onChange={(water_reminder_end_hour) => set({ water_reminder_end_hour })}
            />
          </div>
        ) : null}
      </Section>

      {/* Notifications and startup */}
      <Section title="Notifications and startup">
        <Toggle
          label="Show notifications"
          checked={settings.notifications_enabled}
          onChange={(notifications_enabled) => set({ notifications_enabled })}
        />
        <Toggle
          label="Timer finished"
          checked={settings.notify_pomodoro}
          disabled={!settings.notifications_enabled}
          onChange={(notify_pomodoro) => set({ notify_pomodoro })}
        />
        <Toggle
          label="Water reminders"
          checked={settings.notify_water}
          disabled={!settings.notifications_enabled}
          onChange={(notify_water) => set({ notify_water })}
        />
        <Toggle
          label="Start with Windows"
          hint="Off by default. Adds one startup entry; turning it off removes it."
          checked={settings.start_with_windows}
          onChange={(start_with_windows) => set({ start_with_windows })}
        />
      </Section>

      {/* Notion */}
      <Section title="Notion">
        {notionError ? <ErrorNotice message={notionError} onRetry={() => void loadNotion()} /> : null}

        {!notion?.configured ? (
          <div className="flex flex-col gap-2">
            <p className="text-2xs text-text-subtle">
              Create a public integration at notion.so/my-integrations, then paste its client id and
              secret here. They are stored in the Windows Credential Manager, never in a file.
            </p>
            <TextInput
              label="Client id"
              value={clientId}
              onChange={(e) => setClientId(e.target.value)}
              placeholder="From your Notion integration"
            />
            <TextInput
              label="Client secret"
              type="password"
              value={clientSecret}
              onChange={(e) => setClientSecret(e.target.value)}
              placeholder="From your Notion integration"
            />
            <Button
              size="sm"
              disabled={!clientId.trim() || !clientSecret.trim()}
              onClick={async () => {
                const ok = await configure(clientId, clientSecret);
                if (ok) {
                  setClientId("");
                  setClientSecret("");
                }
              }}
            >
              Save credentials
            </Button>
          </div>
        ) : notion.state === "connected" ? (
          <div className="flex flex-col gap-2">
            <p className="flex items-center gap-1.5 text-xs text-text">
              <PlugZap size={12} className="text-success" aria-hidden />
              Connected{notion.workspace_name ? ` to ${notion.workspace_name}` : ""}
            </p>

            {notion.database_id ? (
              <p className="text-2xs text-text-subtle">
                Using <span className="text-text-muted">{notion.database_name}</span>
              </p>
            ) : (
              <div className="flex flex-col gap-1.5">
                <Button size="sm" onClick={() => void loadDatabases()} disabled={loadingDatabases}>
                  {loadingDatabases ? <Spinner /> : null}
                  Choose a database
                </Button>
                {databases.length > 0 ? (
                  <Select
                    label="Database"
                    value=""
                    options={[
                      { value: "", label: "Select a database" },
                      ...databases.map((d) => ({ value: d.id, label: d.title })),
                    ]}
                    onChange={(e) => void selectDatabase(e.target.value)}
                  />
                ) : null}
              </div>
            )}

            <Button size="sm" variant="ghost" onClick={() => void disconnect()}>
              <Unplug size={12} aria-hidden />
              Disconnect
            </Button>
            <p className="text-2xs text-text-subtle">
              Disconnecting keeps your cached tasks. Nothing is deleted locally.
            </p>
          </div>
        ) : (
          <div className="flex flex-col gap-2">
            <p className="flex items-center gap-1.5 text-xs text-text-muted">
              <Plug size={12} aria-hidden />
              Credentials saved, not connected
            </p>
            <Button size="sm" variant="primary" onClick={() => void connect()} disabled={connecting}>
              {connecting ? <Spinner label="Connecting" /> : null}
              {connecting ? "Waiting for your browser…" : "Connect to Notion"}
            </Button>
          </div>
        )}
      </Section>
    </div>
  );
}

function Section({ title, children }: { title: string; children: React.ReactNode }) {
  return (
    <section className="flex flex-col gap-2 border-b border-border pb-3 last:border-b-0 last:pb-0">
      <h2 className="text-2xs font-medium uppercase tracking-wide text-text-subtle">{title}</h2>
      {children}
    </section>
  );
}

function NumberField({
  label,
  value,
  min,
  max,
  step = 1,
  suffix,
  onChange,
}: {
  label: string;
  value: number;
  min: number;
  max: number;
  step?: number;
  suffix?: string;
  onChange: (value: number) => void;
}) {
  return (
    <label className="flex flex-col gap-1">
      <span className="text-2xs text-text-subtle">
        {label}
        {suffix ? ` (${suffix})` : ""}
      </span>
      <input
        type="number"
        value={value}
        min={min}
        max={max}
        step={step}
        onChange={(e) => {
          const parsed = Number(e.target.value);
          // Only commit values inside the range; the backend clamps too, but
          // refusing here keeps the field from showing a rejected number.
          if (Number.isFinite(parsed) && parsed >= min && parsed <= max) onChange(parsed);
        }}
        className="tabular h-7 rounded-control border border-border bg-surface-sunken px-2 text-xs text-text
          hover:border-border-strong focus:border-accent focus:outline-none"
      />
    </label>
  );
}
