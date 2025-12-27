import { Show, For, createSignal } from 'solid-js';
import { InteractiveQuestion } from '../../stores/interactive';

interface QuestionCardProps {
  question: InteractiveQuestion;
  onSelect: (indices: number[]) => void;
  onCancel: () => void;
}

export function QuestionCard(props: QuestionCardProps) {
  const [selected, setSelected] = createSignal<number[]>([]);

  const handleOptionClick = (index: number) => {
    if (props.question.multiSelect) {
      // Toggle selection for multi-select
      setSelected((prev) =>
        prev.includes(index) ? prev.filter((i) => i !== index) : [...prev, index]
      );
    } else {
      // Single select - immediately submit
      props.onSelect([index]);
    }
  };

  const handleSubmit = () => {
    if (selected().length > 0) {
      props.onSelect(selected());
    }
  };

  return (
    <div class="interactive-card">
      {/* Header label */}
      <div class="interactive-header">{props.question.header}</div>

      {/* Question text */}
      <p class="interactive-question">{props.question.question}</p>

      {/* Options */}
      <div class="interactive-options">
        <For each={props.question.options}>
          {(option) => (
            <button
              class={`interactive-option ${selected().includes(option.index) ? 'selected' : ''}`}
              onClick={() => handleOptionClick(option.index)}
            >
              <div class="option-indicator">
                <Show when={props.question.multiSelect} fallback={<span class="radio-dot" />}>
                  <span class="checkbox-mark">{selected().includes(option.index) ? '✓' : ''}</span>
                </Show>
              </div>
              <div class="option-content">
                <span class="option-label">{option.label}</span>
                <Show when={option.description}>
                  <span class="option-description">{option.description}</span>
                </Show>
              </div>
            </button>
          )}
        </For>
      </div>

      {/* Submit button for multi-select */}
      <Show when={props.question.multiSelect}>
        <button
          class="interactive-submit"
          onClick={handleSubmit}
          disabled={selected().length === 0}
        >
          Submit ({selected().length} selected)
        </button>
      </Show>

      {/* Cancel link */}
      <button class="interactive-cancel" onClick={props.onCancel}>
        Cancel (Esc)
      </button>
    </div>
  );
}
