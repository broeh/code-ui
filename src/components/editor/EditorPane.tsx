import { useEffect, useState } from "react";
import CodeMirror from "@uiw/react-codemirror";
import { languages } from "@codemirror/language-data";
import { LanguageDescription } from "@codemirror/language";
import type { Extension } from "@codemirror/state";
import { Eye, Code, Save, X } from "lucide-react";
import {
  useFiles,
  editText,
  saveEditor,
  reloadEditor,
  closeEditor,
} from "../../store/files";
import { reportError } from "../../store/chats";
import Markdown from "../common/Markdown";
export default function EditorPane() {
  const { editor } = useFiles();
  const [extensions, setExtensions] = useState<Extension[]>([]);
  useEffect(() => {
    let disposed = false;
    setExtensions([]);
    if (editor?.kind === "text") {
      const language = LanguageDescription.matchFilename(languages, editor.rel);
      language
        ?.load()
        .then((support) => {
          if (!disposed) setExtensions([support]);
        })
        .catch(reportError);
    }
    return () => {
      disposed = true;
    };
  }, [editor?.project, editor?.rel, editor?.kind]);
  useEffect(() => {
    const key = (e: KeyboardEvent) => {
      if (e.ctrlKey && e.key.toLowerCase() === "s" && editor?.kind === "text") {
        e.preventDefault();
        saveEditor().catch(reportError);
      }
    };
    window.addEventListener("keydown", key);
    return () => window.removeEventListener("keydown", key);
  }, [editor?.kind]);
  if (!editor) return null;
  const dirty = editor.text !== editor.savedText;
  const markdown = /\.(md|mdx|markdown)$/i.test(editor.rel);
  return (
    <section className="editor-pane">
      <header className="editor-header">
        <span title={`${editor.project}/${editor.rel}`}>
          {editor.rel}
          {dirty && <span className="dirty-dot" />}
        </span>
        <div>
          {markdown && (
            <button
              className="icon-button"
              aria-label={editor.preview ? "Edit markdown" : "Preview markdown"}
              title={editor.preview ? "Edit" : "Preview"}
              onClick={() =>
                useFiles.setState({
                  editor: { ...editor, preview: !editor.preview },
                })
              }
            >
              {editor.preview ? <Code size={15} /> : <Eye size={15} />}
            </button>
          )}
          {editor.kind === "text" && (
            <button
              className="icon-button"
              aria-label="Save file"
              disabled={!dirty || editor.saving}
              onClick={() => saveEditor().catch(reportError)}
            >
              <Save size={15} />
            </button>
          )}
          <button
            className="icon-button"
            aria-label="Close editor"
            onClick={() => closeEditor().catch(reportError)}
          >
            <X size={15} />
          </button>
        </div>
      </header>
      {editor.conflict && (
        <div className="file-conflict" role="alert">
          <span>Changed on disk</span>
          <button onClick={() => reloadEditor().catch(reportError)}>
            Reload
          </button>
          <button
            disabled={editor.saving}
            onClick={() => saveEditor(true).catch(reportError)}
          >
            Overwrite
          </button>
        </div>
      )}
      {editor.kind === "image" ? (
        <div className="image-preview">
          <img src={editor.imageUrl} alt={editor.rel} />
        </div>
      ) : editor.preview ? (
        <div className="markdown-preview">
          <Markdown text={editor.text} />
        </div>
      ) : (
        <CodeMirror
          key={`${editor.project}:${editor.rel}`}
          value={editor.text}
          onChange={(text) => {
            const current = useFiles.getState().editor;
            if (
              current?.project === editor.project &&
              current.rel === editor.rel
            )
              editText(text);
          }}
          extensions={extensions}
          theme="dark"
          className="code-editor"
          basicSetup={{ lineNumbers: true, foldGutter: true }}
          height="100%"
        />
      )}
    </section>
  );
}
