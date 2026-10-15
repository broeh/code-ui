CREATE TABLE projects (
  path TEXT PRIMARY KEY,               -- absolute, canonical
  pinned INTEGER NOT NULL DEFAULT 0,
  hidden INTEGER NOT NULL DEFAULT 0,
  extra INTEGER NOT NULL DEFAULT 0,    -- added via "Add other folder" (outside projectsRoot)
  last_opened_at INTEGER
);

CREATE TABLE chats (
  id TEXT PRIMARY KEY,                 -- uuid v7
  project_path TEXT NOT NULL,
  title TEXT NOT NULL,
  title_locked INTEGER NOT NULL DEFAULT 0,   -- 1 after a manual rename
  active_agent TEXT NOT NULL,          -- claude | codex | agy | muse
  status TEXT NOT NULL DEFAULT 'idle', -- idle | working | needs_input | failed | stopped | interrupted
  unread INTEGER NOT NULL DEFAULT 0,
  hidden INTEGER NOT NULL DEFAULT 0,
  created_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL
);
CREATE INDEX chats_by_project ON chats(project_path, updated_at DESC);

CREATE TABLE agent_sessions (           -- one row per agent that has taken part in a chat
  chat_id TEXT NOT NULL REFERENCES chats(id) ON DELETE CASCADE,
  agent TEXT NOT NULL,
  native_id TEXT,                      -- claude session_id | codex threadId | agy conversation_id | muse sessionId
  synced_seq INTEGER NOT NULL DEFAULT 0,  -- highest message seq this agent's native context contains
  model TEXT, effort TEXT,
  permission TEXT NOT NULL DEFAULT 'ask',  -- yolo | ask | plan
  PRIMARY KEY (chat_id, agent)
);
CREATE UNIQUE INDEX agent_sessions_native ON agent_sessions(agent, native_id);

CREATE TABLE messages (
  chat_id TEXT NOT NULL REFERENCES chats(id) ON DELETE CASCADE,
  seq INTEGER NOT NULL,                -- 1, 2, 3 ... per chat
  role TEXT NOT NULL,                  -- user | assistant | handoff | notice
  agent TEXT,                          -- assistant/handoff: author. user: the agent it was sent to.
  turn_id TEXT,
  text TEXT NOT NULL,                  -- markdown, exactly what the user typed or the agent replied
  meta TEXT NOT NULL DEFAULT '{}',     -- json: model, effort, durationMs, handoff {mode, count}, imported
  created_at INTEGER NOT NULL,
  PRIMARY KEY (chat_id, seq)
);

CREATE TABLE steps (                    -- activity, one row per tool call / thinking / commentary
  id TEXT PRIMARY KEY,                 -- "<turn_id>:<native item id>"
  chat_id TEXT NOT NULL REFERENCES chats(id) ON DELETE CASCADE,
  turn_id TEXT NOT NULL,
  agent TEXT NOT NULL,
  parent_id TEXT,                      -- subagent nesting
  ord INTEGER NOT NULL,
  kind TEXT NOT NULL,                  -- see StepKind
  title TEXT NOT NULL,                 -- the one-liner
  status TEXT NOT NULL,                -- running | done | failed | declined | interrupted
  started_at INTEGER, ended_at INTEGER,
  detail TEXT                          -- json {input, output}, output truncated to 16 KB
);
CREATE INDEX steps_by_turn ON steps(chat_id, turn_id, ord);

CREATE TABLE interactions (             -- question / approval / plan cards
  id TEXT PRIMARY KEY,
  chat_id TEXT NOT NULL REFERENCES chats(id) ON DELETE CASCADE,
  turn_id TEXT NOT NULL,
  agent TEXT NOT NULL,
  kind TEXT NOT NULL,                  -- question | approval | plan
  payload TEXT NOT NULL,               -- normalized json shown in the card
  status TEXT NOT NULL,                -- pending | answered | approved | declined | dismissed | expired
  answer TEXT,
  created_at INTEGER NOT NULL, resolved_at INTEGER
);

CREATE TABLE queue (
  chat_id TEXT NOT NULL REFERENCES chats(id) ON DELETE CASCADE,
  pos INTEGER NOT NULL,
  text TEXT NOT NULL,
  settings TEXT NOT NULL,              -- json TurnSettings incl. agent and handoff mode
  PRIMARY KEY (chat_id, pos)
);

CREATE TABLE native_sessions (          -- cache of terminal chats found on disk (section 11)
  agent TEXT NOT NULL,
  native_id TEXT NOT NULL,
  cwd TEXT NOT NULL,
  title TEXT NOT NULL,
  updated_at INTEGER NOT NULL,
  source_path TEXT NOT NULL,
  stamp TEXT NOT NULL,                 -- mtime/size fingerprint for incremental rescans
  PRIMARY KEY (agent, native_id)
);
CREATE INDEX native_by_cwd ON native_sessions(cwd, updated_at DESC);

CREATE TABLE agent_defaults (           -- "remember last choice per agent"
  agent TEXT PRIMARY KEY, model TEXT, effort TEXT, permission TEXT NOT NULL
);

CREATE TABLE ui_state (key TEXT PRIMARY KEY, value TEXT NOT NULL);
-- keys: open_tabs, active_tab, panel_sizes, shown_panels, last_handoff_mode, show_ignored
