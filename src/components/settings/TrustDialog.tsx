import { useState } from "react";
import { Dialog } from "../common/Dialog";
import { trustAndContinue, useWorkspace } from "../../store/chats";

export default function TrustDialog() {
  const request = useWorkspace((state) => state.trustRequest);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  if (!request) return null;
  return <Dialog title="Trust this project?" confirmLabel="Trust folder and continue" busy={busy} onClose={() => useWorkspace.setState({ trustRequest: null })} onSubmit={async () => {
    setBusy(true); setError(null);
    try { await trustAndContinue(); }
    catch (error) { setError(String(error)); }
    finally { setBusy(false); }
  }}>
    <p>Agent CLIs can load project hooks, plugins and configured tools when they start. Only continue with folders whose contents and configuration you trust.</p>
    <code className="trust-path">{request.project}</code>
    <p>This choice is remembered for this folder. The agent still follows your selected permission mode.</p>
    {error && <p role="alert" className="dialog-error">{error}</p>}
  </Dialog>;
}
