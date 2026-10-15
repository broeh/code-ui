import { Check, Circle, ListChecks, LoaderCircle } from "lucide-react";
import type { Todo } from "../../lib/todos";
export default function TodoPanel({
  todos,
  working,
}: {
  todos: Todo[] | null;
  working: boolean;
}) {
  if (!todos?.length)
    return (
      <div className="small-empty">
        <ListChecks size={22} />
        <span>No tasks yet</span>
        <p>The agent's task list will appear here when it makes one.</p>
      </div>
    );
  return (
    <ul className="todo-list">
      {todos.map((todo, index) => (
        <li key={index} className={`todo ${todo.state}`}>
          {todo.state === "done" ? (
            <Check />
          ) : todo.state === "active" && working ? (
            <LoaderCircle className="spinner" />
          ) : (
            <Circle />
          )}
          <span>{todo.text}</span>
        </li>
      ))}
    </ul>
  );
}
