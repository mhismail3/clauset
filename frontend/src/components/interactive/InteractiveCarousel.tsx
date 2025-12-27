import { Show, For, createSignal, createEffect, onCleanup } from 'solid-js';
import {
  InteractiveQuestion,
  PromptSession,
  setQuestionAnswer,
  setCurrentQuestionIndex,
  goToNextQuestion,
  goToPreviousQuestion,
  allQuestionsAnswered,
  getOrderedAnswers,
  clearInteractiveState,
} from '../../stores/interactive';

interface InteractiveCarouselProps {
  sessionId: string;
  session: PromptSession;
  onSubmitAll: (answers: Array<{ questionId: string; selectedIndices: number[] }>) => void;
  onCancel: () => void;
}

export function InteractiveCarousel(props: InteractiveCarouselProps) {
  const [touchStart, setTouchStart] = createSignal<number | null>(null);
  const [touchDelta, setTouchDelta] = createSignal(0);
  const [isAnimating, setIsAnimating] = createSignal(false);

  // Current question based on carousel index
  const currentQuestion = () => props.session.prompt.questions[props.session.currentIndex];
  const totalQuestions = () => props.session.prompt.questions.length;

  // Check if current question has an answer
  const currentAnswer = () => props.session.answers.get(currentQuestion()?.id ?? '');
  const hasCurrentAnswer = () => (currentAnswer()?.length ?? 0) > 0;

  // Handle touch swipe navigation
  const handleTouchStart = (e: TouchEvent) => {
    if (isAnimating()) return;
    setTouchStart(e.touches[0].clientX);
    setTouchDelta(0);
  };

  const handleTouchMove = (e: TouchEvent) => {
    if (touchStart() === null || isAnimating()) return;
    const delta = e.touches[0].clientX - touchStart()!;
    setTouchDelta(delta);
  };

  const handleTouchEnd = () => {
    if (touchStart() === null || isAnimating()) return;

    const delta = touchDelta();
    const threshold = 50;

    if (delta > threshold && props.session.currentIndex > 0) {
      goToPreviousQuestion(props.sessionId);
    } else if (delta < -threshold && props.session.currentIndex < totalQuestions() - 1) {
      goToNextQuestion(props.sessionId);
    }

    setTouchStart(null);
    setTouchDelta(0);
  };

  // Handle keyboard navigation
  const handleKeyDown = (e: KeyboardEvent) => {
    if (e.key === 'ArrowLeft' && props.session.currentIndex > 0) {
      e.preventDefault();
      goToPreviousQuestion(props.sessionId);
    } else if (e.key === 'ArrowRight' && props.session.currentIndex < totalQuestions() - 1) {
      e.preventDefault();
      goToNextQuestion(props.sessionId);
    } else if (e.key === 'Escape') {
      e.preventDefault();
      props.onCancel();
    }
  };

  createEffect(() => {
    window.addEventListener('keydown', handleKeyDown);
    onCleanup(() => window.removeEventListener('keydown', handleKeyDown));
  });

  // Handle option selection for current question
  const handleOptionSelect = (optionIndex: number) => {
    const q = currentQuestion();
    if (!q) return;

    if (q.multiSelect) {
      // Toggle selection for multi-select
      const current = currentAnswer() ?? [];
      const newSelection = current.includes(optionIndex)
        ? current.filter((i) => i !== optionIndex)
        : [...current, optionIndex];
      setQuestionAnswer(props.sessionId, q.id, newSelection);
    } else {
      // Single select - set answer and auto-advance if not last question
      setQuestionAnswer(props.sessionId, q.id, [optionIndex]);

      // Auto-advance to next question after short delay
      if (props.session.currentIndex < totalQuestions() - 1) {
        setIsAnimating(true);
        setTimeout(() => {
          goToNextQuestion(props.sessionId);
          setIsAnimating(false);
        }, 300);
      }
    }
  };

  // Confirm multi-select answer and advance
  const handleConfirmMultiSelect = () => {
    if (props.session.currentIndex < totalQuestions() - 1) {
      goToNextQuestion(props.sessionId);
    }
  };

  // Submit all answers
  const handleSubmitAll = () => {
    const answers = getOrderedAnswers(props.sessionId);
    props.onSubmitAll(answers);
    clearInteractiveState(props.sessionId);
  };

  // Calculate card transform based on touch delta
  const cardStyle = () => {
    const delta = touchDelta();
    if (delta === 0) return {};
    return {
      transform: `translateX(${delta}px)`,
      transition: 'none',
    };
  };

  return (
    <div class="interactive-carousel" onTouchStart={handleTouchStart} onTouchMove={handleTouchMove} onTouchEnd={handleTouchEnd}>
      {/* Navigation dots */}
      <div class="carousel-dots">
        <For each={props.session.prompt.questions}>
          {(q, i) => (
            <button
              class={`carousel-dot ${i() === props.session.currentIndex ? 'active' : ''} ${props.session.answers.has(q.id) ? 'answered' : ''}`}
              onClick={() => setCurrentQuestionIndex(props.sessionId, i())}
              title={q.header}
            >
              <Show when={props.session.answers.has(q.id)}>
                <span class="dot-check">✓</span>
              </Show>
            </button>
          )}
        </For>
      </div>

      {/* Progress indicator */}
      <div class="carousel-progress">
        {props.session.currentIndex + 1} / {totalQuestions()}
      </div>

      {/* Question card */}
      <Show when={currentQuestion()}>
        {(q) => (
          <div class="interactive-card" style={cardStyle()}>
            <div class="interactive-header">{q().header}</div>
            <p class="interactive-question">{q().question}</p>

            <div class="interactive-options">
              <For each={q().options}>
                {(option) => {
                  const isSelected = () => (currentAnswer() ?? []).includes(option.index);
                  return (
                    <button
                      class={`interactive-option ${isSelected() ? 'selected' : ''}`}
                      onClick={() => handleOptionSelect(option.index)}
                    >
                      <div class="option-indicator">
                        <Show when={q().multiSelect} fallback={<span class="radio-dot" />}>
                          <span class="checkbox-mark">{isSelected() ? '✓' : ''}</span>
                        </Show>
                      </div>
                      <div class="option-content">
                        <span class="option-label">{option.label}</span>
                        <Show when={option.description}>
                          <span class="option-description">{option.description}</span>
                        </Show>
                      </div>
                    </button>
                  );
                }}
              </For>
            </div>

            {/* Confirm button for multi-select */}
            <Show when={q().multiSelect && hasCurrentAnswer()}>
              <button class="interactive-confirm" onClick={handleConfirmMultiSelect}>
                <Show when={props.session.currentIndex < totalQuestions() - 1} fallback="Done">
                  Next →
                </Show>
              </button>
            </Show>
          </div>
        )}
      </Show>

      {/* Navigation arrows */}
      <div class="carousel-nav">
        <button
          class="nav-arrow prev"
          disabled={props.session.currentIndex === 0}
          onClick={() => goToPreviousQuestion(props.sessionId)}
        >
          ←
        </button>
        <button
          class="nav-arrow next"
          disabled={props.session.currentIndex === totalQuestions() - 1}
          onClick={() => goToNextQuestion(props.sessionId)}
        >
          →
        </button>
      </div>

      {/* Submit all button - appears when all questions answered */}
      <Show when={allQuestionsAnswered(props.sessionId)}>
        <button class="interactive-submit-all" onClick={handleSubmitAll}>
          Send All Responses ({props.session.answers.size})
        </button>
      </Show>

      {/* Cancel button */}
      <button class="interactive-cancel" onClick={props.onCancel}>
        Cancel (Esc)
      </button>
    </div>
  );
}
