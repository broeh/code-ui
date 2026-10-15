import { useState } from "react";
import type { InteractionRow, InteractionResponse } from "../../ipc/types";
import { respondInteraction } from "../../ipc/commands";
import { reportError, useWorkspace, setTurnSettings } from "../../store/chats";
import Markdown from "../common/Markdown";
export default function InteractionCard({ row }: { row: InteractionRow }) {
  const { snapshot, activeTab, settings } = useWorkspace();
  const [answers, setAnswers] = useState<Record<string, string[]>>({});
  const [other, setOther] = useState<Record<string, string>>({});
  const [note, setNote] = useState("");
  const [busy, setBusy] = useState(false);
  const [feedback, setFeedback] = useState(false);
  const card = row.payload;
  async function respond(response: InteractionResponse) {
    if (!snapshot || busy) return;
    setBusy(true);
    try {
      await respondInteraction(snapshot.id, row.id, response);
    } catch (error) {
      reportError(error);
    } finally {
      setBusy(false);
    }
  }
  if (row.status !== "pending")
    return (
      <details className="resolved-card">
        <summary>
          {card.kind === "question"
            ? `${card.questions.length} ${card.questions.length === 1 ? "question" : "questions"}`
            : "1 decision"}{" "}
          · {row.status}
        </summary>
        <p>{card.title}</p>
        {row.answer && (
          <pre>{JSON.stringify(JSON.parse(row.answer), null, 2)}</pre>
        )}
      </details>
    );
  return (
    <section
      className={`interaction-card ${card.kind}`}
      aria-label={card.title}
    >
      <h3>{card.title}</h3>
      {card.kind === "question" && (
        <>
          {card.questions.map((q) => (
            <fieldset key={q.id}>
              <legend>
                <span className="question-header">{q.header}</span>
                {q.text}
              </legend>
              <div className="question-options">
                {q.options.map((option) => {
                  const selected = (answers[q.id] ?? []).includes(option.label);
                  return (
                    <button
                      key={option.label}
                      className={selected ? "option selected" : "option"}
                      aria-pressed={selected}
                      onClick={() => {
                        setOther({ ...other, [q.id]: "" });
                        setAnswers({
                          ...answers,
                          [q.id]: q.multi
                            ? selected
                              ? (answers[q.id] ?? []).filter(
                                  (v) => v !== option.label,
                                )
                              : [...(answers[q.id] ?? []), option.label]
                            : [option.label],
                        });
                      }}
                    >
                      <strong>
                        {q.multi ? (
                          <input
                            type="checkbox"
                            checked={selected}
                            readOnly
                            tabIndex={-1}
                          />
                        ) : null}
                        {option.label}
                      </strong>
                      <small>{option.description}</small>
                    </button>
                  );
                })}
              </div>
              {q.allowOther && (
                <input
                  aria-label={`Something else: ${q.text}`}
                  type={q.secret ? "password" : "text"}
                  maxLength={card.maxTextLength}
                  placeholder="Something else"
                  value={other[q.id] ?? ""}
                  onChange={(e) => {
                    setOther({ ...other, [q.id]: e.target.value });
                    setAnswers({ ...answers, [q.id]: [] });
                  }}
                />
              )}
            </fieldset>
          ))}
          <textarea
            aria-label="Anything else?"
            maxLength={card.maxTextLength}
            placeholder="Anything else?"
            value={note}
            onChange={(e) => setNote(e.target.value)}
            rows={2}
          />
          <div className="card-actions">
            <button
              className="primary"
              disabled={
                busy ||
                card.questions.some(
                  (q) => !(answers[q.id]?.length || other[q.id]?.trim()),
                )
              }
              onClick={() =>
                respond({
                  type: "answers",
                  answers: card.questions.map((q) => ({
                    questionId: q.id,
                    labels: answers[q.id] ?? [],
                    other: other[q.id] || null,
                  })),
                  note: note || null,
                })
              }
            >
              Send answers
            </button>
            {card.canDismiss !== false && (
              <button
                disabled={busy}
                onClick={() => respond({ type: "dismiss" })}
              >
                Dismiss
              </button>
            )}
          </div>
        </>
      )}
      {card.kind === "approval" && card.approval && (
        <>
          <p>{card.approval.summary}</p>
          <pre>{card.approval.detail}</pre>
          <div className="card-actions">
            {card.approval.choices.map((choice) => (
              <button
                key={choice.id}
                className={
                  choice.kind === "allow"
                    ? "primary"
                    : choice.kind === "deny"
                      ? "ghost"
                      : ""
                }
                disabled={busy}
                onClick={() =>
                  respond({
                    type: "choice",
                    choiceId: choice.id,
                    feedback: null,
                  })
                }
              >
                {choice.label}
              </button>
            ))}
          </div>
        </>
      )}
      {card.kind === "plan" && (
        <>
          <Markdown text={card.plan ?? ""} />
          {feedback && (
            <textarea
              aria-label="Plan feedback"
              placeholder="What should change?"
              value={note}
              onChange={(e) => setNote(e.target.value)}
            />
          )}
          <div className="card-actions">
            <button
              className="primary"
              disabled={busy}
              onClick={() => {
                setTurnSettings(
                  { ...settings, permission: "ask" },
                  activeTab,
                ).catch(reportError);
                respond({ type: "choice", choiceId: "allow", feedback: null });
              }}
            >
              Start working with approvals
            </button>
            <button
              disabled={busy}
              onClick={() =>
                feedback
                  ? respond({
                      type: "choice",
                      choiceId: "deny",
                      feedback: note || "Keep planning.",
                    })
                  : setFeedback(true)
              }
            >
              {feedback ? "Send feedback" : "Keep planning"}
            </button>
          </div>
        </>
      )}
    </section>
  );
}
