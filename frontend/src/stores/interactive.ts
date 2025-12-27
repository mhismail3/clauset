/**
 * Interactive prompts store for native UI rendering.
 *
 * Handles interactive questions from Claude Code's AskUserQuestion tool,
 * displaying them as native UI components instead of raw terminal output.
 * Supports multi-question prompts with swipeable card carousel.
 */
import { createSignal } from 'solid-js';

// Types matching the backend
export interface InteractiveQuestion {
  id: string;
  header: string;
  question: string;
  options: QuestionOption[];
  multiSelect: boolean;
}

export interface QuestionOption {
  index: number;
  label: string;
  description?: string;
}

export interface InteractivePrompt {
  id: string;
  questions: InteractiveQuestion[];
}

// Answer for a single question
export interface QuestionAnswer {
  questionId: string;
  selectedIndices: number[];
}

// State for a multi-question prompt session
export interface PromptSession {
  prompt: InteractivePrompt;
  currentIndex: number;
  answers: Map<string, number[]>; // questionId -> selected indices
}

export type InteractiveState =
  | { type: 'idle' }
  | { type: 'prompt'; session: PromptSession };

export type InteractiveEvent =
  | { type: 'prompt_presented'; session_id: string; prompt: InteractivePrompt }
  | { type: 'interaction_complete'; session_id: string };

// Store: Map of session ID to interactive state
const [interactiveStates, setInteractiveStates] = createSignal<Map<string, InteractiveState>>(
  new Map()
);

/**
 * Get the current interactive state for a session.
 */
export function getInteractiveState(sessionId: string): InteractiveState {
  return interactiveStates().get(sessionId) ?? { type: 'idle' };
}

/**
 * Handle an interactive event from the WebSocket.
 */
export function handleInteractiveEvent(event: InteractiveEvent) {
  console.log('[interactive]', event.type, event);

  switch (event.type) {
    case 'prompt_presented': {
      // Convert snake_case to camelCase for questions
      const questions: InteractiveQuestion[] = event.prompt.questions.map((q: any) => ({
        id: q.id,
        header: q.header,
        question: q.question,
        multiSelect: q.multi_select ?? q.multiSelect ?? false,
        options: q.options.map((opt: any) => ({
          index: opt.index,
          label: opt.label,
          description: opt.description,
        })),
      }));

      const prompt: InteractivePrompt = {
        id: event.prompt.id,
        questions,
      };

      const session: PromptSession = {
        prompt,
        currentIndex: 0,
        answers: new Map(),
      };

      setInteractiveStates((prev) => {
        const next = new Map(prev);
        next.set(event.session_id, { type: 'prompt', session });
        return next;
      });
      break;
    }

    case 'interaction_complete': {
      setInteractiveStates((prev) => {
        const next = new Map(prev);
        next.set(event.session_id, { type: 'idle' });
        return next;
      });
      break;
    }
  }
}

/**
 * Set the answer for a specific question.
 */
export function setQuestionAnswer(sessionId: string, questionId: string, selectedIndices: number[]) {
  setInteractiveStates((prev) => {
    const state = prev.get(sessionId);
    if (state?.type !== 'prompt') return prev;

    const next = new Map(prev);
    const newAnswers = new Map(state.session.answers);
    newAnswers.set(questionId, selectedIndices);

    next.set(sessionId, {
      type: 'prompt',
      session: {
        ...state.session,
        answers: newAnswers,
      },
    });
    return next;
  });
}

/**
 * Navigate to a specific question in the carousel.
 */
export function setCurrentQuestionIndex(sessionId: string, index: number) {
  setInteractiveStates((prev) => {
    const state = prev.get(sessionId);
    if (state?.type !== 'prompt') return prev;

    const next = new Map(prev);
    next.set(sessionId, {
      type: 'prompt',
      session: {
        ...state.session,
        currentIndex: Math.max(0, Math.min(index, state.session.prompt.questions.length - 1)),
      },
    });
    return next;
  });
}

/**
 * Navigate to the next question in the carousel.
 */
export function goToNextQuestion(sessionId: string) {
  const state = getInteractiveState(sessionId);
  if (state.type !== 'prompt') return;
  setCurrentQuestionIndex(sessionId, state.session.currentIndex + 1);
}

/**
 * Navigate to the previous question in the carousel.
 */
export function goToPreviousQuestion(sessionId: string) {
  const state = getInteractiveState(sessionId);
  if (state.type !== 'prompt') return;
  setCurrentQuestionIndex(sessionId, state.session.currentIndex - 1);
}

/**
 * Get the number of answered questions.
 */
export function getAnsweredCount(sessionId: string): number {
  const state = getInteractiveState(sessionId);
  if (state.type !== 'prompt') return 0;
  return state.session.answers.size;
}

/**
 * Check if all questions are answered.
 */
export function allQuestionsAnswered(sessionId: string): boolean {
  const state = getInteractiveState(sessionId);
  if (state.type !== 'prompt') return false;
  return state.session.answers.size === state.session.prompt.questions.length;
}

/**
 * Get all answers in order for sending to terminal.
 */
export function getOrderedAnswers(sessionId: string): Array<{ questionId: string; selectedIndices: number[] }> {
  const state = getInteractiveState(sessionId);
  if (state.type !== 'prompt') return [];

  return state.session.prompt.questions.map((q) => ({
    questionId: q.id,
    selectedIndices: state.session.answers.get(q.id) ?? [],
  }));
}

/**
 * Clear the interactive state for a session.
 * Called after user submits all responses or cancels.
 */
export function clearInteractiveState(sessionId: string) {
  setInteractiveStates((prev) => {
    const next = new Map(prev);
    next.set(sessionId, { type: 'idle' });
    return next;
  });
}

/**
 * Check if a session has an active interactive prompt.
 */
export function hasActivePrompt(sessionId: string): boolean {
  const state = getInteractiveState(sessionId);
  return state.type !== 'idle';
}

export { interactiveStates };
