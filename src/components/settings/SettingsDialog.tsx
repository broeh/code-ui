import { useState } from "react";
import { isTauri } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import { Dialog } from "../common/Dialog";
import { refreshAgents, saveSettings } from "../../ipc/commands";
import { agentNames, type AgentKind, type AppSettings } from "../../ipc/types";
import { applySettings, useWorkspace } from "../../store/chats";
import AgentIcon from "../common/AgentIcon";
import { availableAgent } from "../../lib/agents";

const agents: AgentKind[] = ["claude", "codex", "agy", "muse"];
const steps = ["Project folder", "Coding agents", "Preferences"];

export default function SettingsDialog({ setup, onClose }: { setup: boolean; onClose: () => void }) {
  const workspace = useWorkspace();
  const [settings, setSettings] = useState<AppSettings>(() => {
    const settings = structuredClone(workspace.appSettings!);
    if (setup) settings.defaultAgent = availableAgent(workspace.agents, settings.defaultAgent);
    return settings;
  });
  const [detected, setDetected] = useState(workspace.agents);
  const [step, setStep] = useState(0);
  const [createRoot, setCreateRoot] = useState(false);
  const [busy, setBusy] = useState(false);
  const [checking, setChecking] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const update = (patch: Partial<AppSettings>) => setSettings((current) => ({ ...current, ...patch }));

  async function browse(agent?: AgentKind) {
    try {
      const path = await open({ title: agent ? `Choose ${agentNames[agent]} executable` : "Choose projects folder", directory: !agent, multiple: false });
      if (path) {
        if (agent) update({ binaries: { ...settings.binaries, [agent]: path } });
        else { update({ projectsRoot: path }); setCreateRoot(false); }
      }
    } catch (error) { setError(String(error)); }
  }

  function enable(agent: AgentKind, enabled: boolean) {
    const enabledAgents = enabled ? [...settings.enabledAgents, agent] : settings.enabledAgents.filter((item) => item !== agent);
    update({
      enabledAgents,
      defaultAgent: enabledAgents.includes(settings.defaultAgent) ? settings.defaultAgent : enabledAgents[0] ?? "claude",
      historyAgents: settings.historyAgents.filter((item) => enabledAgents.includes(item)),
    });
  }

  async function check() {
    setChecking(true); setError(null);
    try { setDetected(await refreshAgents(settings)); }
    catch (error) { setError(String(error)); }
    finally { setChecking(false); }
  }

  async function submit() {
    if (step < 2) { setStep(step + 1); return; }
    setBusy(true); setError(null);
    try {
      const saved = await saveSettings({ ...settings, setupComplete: true }, createRoot);
      await applySettings(saved);
      onClose();
    } catch (error) { setError(String(error)); }
    finally { setBusy(false); }
  }

  return (
    <Dialog
      title={setup ? "Set up CODE_UI" : "Settings"}
      className="settings-dialog"
      confirmLabel={step < 2 ? "Next" : setup ? "Open workspace" : "Save settings"}
      confirmDisabled={checking || (step === 0 && !settings.projectsRoot.trim())}
      busy={busy}
      dismissible={!setup}
      onClose={onClose}
      onSubmit={() => void submit()}
      footerStart={step > 0 ? <button className="settings-back" type="button" disabled={busy} onClick={() => setStep(step - 1)}>Back</button> : undefined}
    >
      <nav className="setup-steps" aria-label="Setup steps">
        {steps.map((name, index) => <button type="button" key={name} aria-current={step === index ? "step" : undefined} onClick={() => setStep(index)} disabled={busy}>{index + 1}. {name}</button>)}
      </nav>
      {workspace.settingsError && <p role="alert" className="dialog-error">{workspace.settingsError} Saving will keep a private backup of the original file.</p>}
      <div className="settings-body">
        {step === 0 && <>
          <h3>Your projects</h3>
          <p>Choose the folder containing your projects. You can add other folders from the Projects panel at any time.</p>
          <label className="dialog-field">Projects folder
            <div className="settings-path"><input value={settings.projectsRoot} onChange={(event) => update({ projectsRoot: event.target.value })} />{isTauri() && <button type="button" onClick={() => void browse()}>Browse</button>}</div>
          </label>
          <label className="settings-check"><input type="checkbox" checked={createRoot} onChange={(event) => setCreateRoot(event.target.checked)} />Create this folder if it does not exist</label>
          <p className="settings-note">Opening a folder does not run an agent. You will be asked to trust it before the first agent action.</p>
        </>}
        {step === 1 && <>
          <div className="settings-heading"><h3>Your coding agents</h3><button type="button" disabled={checking || busy} onClick={() => void check()}>{checking ? "Checking…" : "Check again"}</button></div>
          <p>Enable the CLIs you use. Install and sign in through each provider's official CLI, then check again. Detection checks the executable and version, not your login.</p>
          <div className="settings-agents">
            {agents.map((agent) => {
              const status = detected.find((item) => item.agent === agent);
              return <section key={agent} className="settings-agent">
                <label className="settings-check"><input type="checkbox" checked={settings.enabledAgents.includes(agent)} onChange={(event) => enable(agent, event.target.checked)} /><AgentIcon agent={agent} /><strong>{agentNames[agent]}</strong>{["agy", "muse"].includes(agent) && <span className="experimental">Experimental</span>}</label>
                <small>{status?.problem ?? status?.version ?? (status?.installed ? "Executable found" : "Not detected")}</small>
                <label className="dialog-field">Executable path
                  <div className="settings-path"><input aria-label={`${agentNames[agent]} executable path`} placeholder="Automatic detection" value={settings.binaries[agent] ?? ""} onChange={(event) => update({ binaries: { ...settings.binaries, [agent]: event.target.value || null } })} />{isTauri() && <button type="button" onClick={() => void browse(agent)}>Browse</button>}</div>
                </label>
              </section>;
            })}
          </div>
          <p className="settings-note">You can finish setup without a CLI and browse your projects. Chat becomes available after an enabled CLI is detected.</p>
        </>}
        {step === 2 && <>
          <h3>Start with your preferred agent</h3>
          <label className="dialog-field">Default agent
            <select aria-label="Default agent" value={settings.defaultAgent} disabled={!settings.enabledAgents.length} onChange={(event) => update({ defaultAgent: event.target.value as AgentKind })}>
              {settings.enabledAgents.length ? settings.enabledAgents.map((agent) => <option key={agent} value={agent}>{agentNames[agent]}</option>) : <option value="claude">Choose later</option>}
            </select>
          </label>
          <fieldset className="settings-permissions"><legend>Default permission</legend>
            <label><input type="radio" name="permission" checked={settings.defaultPermission === "ask"} onChange={() => update({ defaultPermission: "ask" })} /><span><strong>Ask</strong><small>Use the CLI's approval controls. Behavior differs by agent.</small></span></label>
            <label><input type="radio" name="permission" checked={settings.defaultPermission === "yolo"} onChange={() => update({ defaultPermission: "yolo" })} /><span><strong>Full access</strong><small>Agents can run commands and change files without approval, including outside the project where the CLI permits it.</small></span></label>
          </fieldset>
          <h3>Existing CLI conversations</h3>
          <p>Choose which local histories CODE_UI may discover. Opening a conversation copies its available text into the app and links its native session. Later terminal messages are not automatically synchronized.</p>
          {settings.enabledAgents.map((agent) => <label className="settings-check" key={agent}><input type="checkbox" checked={settings.historyAgents.includes(agent)} onChange={(event) => update({ historyAgents: event.target.checked ? [...settings.historyAgents, agent] : settings.historyAgents.filter((item) => item !== agent) })} />Discover {agentNames[agent]} history</label>)}
          <label className="settings-check"><input type="checkbox" checked={settings.notifications} onChange={(event) => update({ notifications: event.target.checked })} />Desktop notifications when the app is in the background</label>
          <p className="settings-note">Conversations are stored locally in plain text. Agent CLIs send requests to their providers under your own account and usage limits. Existing chat permissions stay unchanged.</p>
        </>}
      </div>
      {error && <p role="alert" className="dialog-error">{error}</p>}
    </Dialog>
  );
}
