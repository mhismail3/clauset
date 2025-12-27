import { Show, For, createSignal } from 'solid-js';

export interface TodoItem {
  content: string;
  status: 'pending' | 'in_progress' | 'completed';
  activeForm?: string;
}

interface TodoWidgetProps {
  todos: TodoItem[];
}

export function TodoWidget(props: TodoWidgetProps) {
  const [expanded, setExpanded] = createSignal(true);

  const hasTodos = () => props.todos.length > 0;
  const pendingCount = () => props.todos.filter((t) => t.status === 'pending').length;
  const inProgressCount = () => props.todos.filter((t) => t.status === 'in_progress').length;
  const completedCount = () => props.todos.filter((t) => t.status === 'completed').length;

  const statusIcon = (status: TodoItem['status']) => {
    switch (status) {
      case 'completed':
        return '✓';
      case 'in_progress':
        return '◐';
      default:
        return '○';
    }
  };

  const statusColor = (status: TodoItem['status']) => {
    switch (status) {
      case 'completed':
        return '#22c55e';
      case 'in_progress':
        return '#3b82f6';
      default:
        return 'var(--color-text-muted)';
    }
  };

  return (
    <Show when={hasTodos()}>
      <div
        style={{
          background: 'var(--color-bg-surface)',
          border: '1px solid var(--color-bg-overlay)',
          'border-radius': '8px',
          margin: '8px 16px',
          overflow: 'hidden',
          'box-shadow': '1px 1px 0px rgba(0, 0, 0, 0.15)',
        }}
      >
        {/* Header */}
        <button
          onClick={() => setExpanded(!expanded())}
          style={{
            width: '100%',
            display: 'flex',
            'align-items': 'center',
            'justify-content': 'space-between',
            padding: '8px 12px',
            background: 'transparent',
            border: 'none',
            cursor: 'pointer',
            'font-family': 'var(--font-mono)',
            'font-size': '12px',
            color: 'var(--color-text-primary)',
          }}
        >
          <div style={{ display: 'flex', 'align-items': 'center', gap: '8px' }}>
            <span>☑</span>
            <span style={{ 'font-weight': '600' }}>Tasks</span>
            <div style={{ display: 'flex', gap: '6px', 'font-size': '11px' }}>
              <Show when={completedCount() > 0}>
                <span style={{ color: '#22c55e' }}>{completedCount()} done</span>
              </Show>
              <Show when={inProgressCount() > 0}>
                <span style={{ color: '#3b82f6' }}>{inProgressCount()} active</span>
              </Show>
              <Show when={pendingCount() > 0}>
                <span style={{ color: 'var(--color-text-muted)' }}>{pendingCount()} pending</span>
              </Show>
            </div>
          </div>
          <svg
            width="12"
            height="12"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            stroke-width="2"
            style={{
              color: 'var(--color-text-muted)',
              transform: expanded() ? 'rotate(180deg)' : 'rotate(0deg)',
              transition: 'transform 0.15s ease',
            }}
          >
            <path d="M6 9l6 6 6-6" />
          </svg>
        </button>

        {/* Task list */}
        <Show when={expanded()}>
          <div
            style={{
              padding: '0 12px 8px',
              'max-height': '200px',
              'overflow-y': 'auto',
            }}
          >
            <For each={props.todos}>
              {(todo) => (
                <div
                  style={{
                    display: 'flex',
                    'align-items': 'flex-start',
                    gap: '8px',
                    padding: '4px 0',
                    'font-size': '12px',
                    'font-family': 'var(--font-mono)',
                  }}
                >
                  <span
                    style={{
                      color: statusColor(todo.status),
                      'flex-shrink': '0',
                      'margin-top': '1px',
                    }}
                  >
                    {statusIcon(todo.status)}
                  </span>
                  <span
                    style={{
                      color:
                        todo.status === 'completed'
                          ? 'var(--color-text-muted)'
                          : 'var(--color-text-primary)',
                      'text-decoration': todo.status === 'completed' ? 'line-through' : 'none',
                      opacity: todo.status === 'completed' ? 0.7 : 1,
                    }}
                  >
                    {todo.status === 'in_progress' && todo.activeForm
                      ? todo.activeForm
                      : todo.content}
                  </span>
                </div>
              )}
            </For>
          </div>
        </Show>
      </div>
    </Show>
  );
}
