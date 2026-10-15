import { useState } from "react";
import { LoaderCircle, MessageSquare, Plus, Search } from "lucide-react";
import PanelHeader from "../common/PanelHeader";
import ContextMenu from "../common/ContextMenu";
import { NameDialog } from "../common/Dialog";
import { hideChat, openChat, renameChat } from "../../ipc/commands";
import {
  newChat,
  refreshChat,
  reportError,
  openChatRow,
  appChatId,
  useWorkspace,
} from "../../store/chats";
import { agentNames, type ChatRow } from "../../ipc/types";
import AgentIcon from "../common/AgentIcon";
import { relativeTime } from "../../lib/format";
function statusPill(chat: ChatRow) {
  if (chat.kind === "terminal") return null;
  switch (chat.status) {
    case "needs_input":
      return <span className="pill needs">Needs you</span>;
    case "working":
      return (
        <span className="pill working">
          <LoaderCircle className="spinner" />
          Working
        </span>
      );
    case "failed":
      return <span className="pill failed">Failed</span>;
    case "stopped":
    case "interrupted":
      return <span className="pill muted">{chat.status}</span>;
    default:
      return chat.unread ? <span className="pill unread">Unread</span> : null;
  }
}
export default function ChatList({
  collapsed,
  onToggle,
}: {
  collapsed: boolean;
  onToggle: () => void;
}) {
  const { chats, snapshot, project } = useWorkspace();
  const [search, setSearch] = useState("");
  const [editing, setEditing] = useState<ChatRow | null>(null);
  const [menu, setMenu] = useState<{
    x: number;
    y: number;
    chat: ChatRow;
  } | null>(null);
  const visible = chats.filter((chat) =>
    chat.title.toLowerCase().includes(search.toLowerCase()),
  );
  return (
    <section className={`panel ${collapsed ? "collapsed" : ""}`}>
      <PanelHeader
        collapsed={collapsed}
        onToggle={onToggle}
        actions={
          <button
            className="icon-button"
            aria-label="New chat"
            disabled={!project}
            onClick={() => newChat().catch(reportError)}
          >
            <Plus size={16} />
          </button>
        }
      >
        Chats <span className="count">{chats.length}</span>
      </PanelHeader>
      <label className="search-field">
        <Search size={14} />
        <input
          placeholder="Find a chat"
          aria-label="Find a chat"
          value={search}
          onChange={(e) => setSearch(e.target.value)}
        />
      </label>
      {visible.length ? (
        <div className="chat-list">
          {visible.map((chat) => (
            <button
              key={chat.id}
              className={
                snapshot?.id === chat.id ? "chat-row selected" : "chat-row"
              }
              title={chat.title}
              onClick={() => openChatRow(chat).catch(reportError)}
              onContextMenu={(e) => {
                e.preventDefault();
                setMenu({ x: e.clientX, y: e.clientY, chat });
              }}
            >
              <span className={`agent-chip ${chat.agent}`}>
                <AgentIcon agent={chat.agent} />
              </span>
              <span className="chat-row-text">
                <span>{chat.title}</span>
                <small className="chat-meta">
                  <span>
                    {agentNames[chat.agent].replace(" Code", "")} ·{" "}
                    {relativeTime(chat.updatedAt)}
                  </span>
                  {(chat.kind === "terminal" || chat.imported) && (
                    <span className="terminal-tag">terminal</span>
                  )}
                </small>
              </span>
              {statusPill(chat)}
            </button>
          ))}
        </div>
      ) : (
        <div className="small-empty">
          <MessageSquare size={22} />
          <span>{search ? "No matching chats" : "No chats yet"}</span>
          <p>Conversations stay with their project.</p>
        </div>
      )}
      {menu && (
        <ContextMenu
          x={menu.x}
          y={menu.y}
          onClose={() => setMenu(null)}
          items={[
            {
              label: "Open",
              action: () => openChatRow(menu.chat).catch(reportError),
            },
            { label: "Rename", action: () => setEditing(menu.chat) },
            {
              label: "Remove from list",
              action: () =>
                appChatId(menu.chat)
                  .then(hideChat)
                  .then(() => refreshChat())
                  .catch(reportError),
            },
            {
              label: "Copy session id",
              action: async () => {
                try {
                  if (menu.chat.kind === "terminal") {
                    await navigator.clipboard.writeText(menu.chat.nativeId!);
                    return;
                  }
                  const current = await openChat(menu.chat.id);
                  const native = current.sessions.find(
                    (s) => s.agent === current.activeAgent,
                  )?.nativeId;
                  await navigator.clipboard.writeText(native ?? current.id);
                } catch (error) {
                  reportError(error);
                }
              },
            },
          ]}
        />
      )}{" "}
      {editing && (
        <NameDialog
          title="Rename chat"
          label="Chat title"
          initial={editing.title}
          onClose={() => setEditing(null)}
          submit={async (title) => {
            const id = await appChatId(editing);
            await renameChat(id, title);
            await refreshChat(id);
          }}
        />
      )}
    </section>
  );
}
