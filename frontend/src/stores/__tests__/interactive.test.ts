import { describe, it, expect, beforeEach } from 'vitest';
import {
  getInteractiveState,
  handleInteractiveEvent,
  setQuestionAnswer,
  setCurrentQuestionIndex,
  goToNextQuestion,
  goToPreviousQuestion,
  getAnsweredCount,
  allQuestionsAnswered,
  getOrderedAnswers,
  clearInteractiveState,
  hasActivePrompt,
  interactiveStates,
  type InteractivePrompt,
  type InteractiveEvent,
} from '../interactive';

// Helper to create a test prompt
function createTestPrompt(numQuestions: number = 2): InteractivePrompt {
  const questions = [];
  for (let i = 0; i < numQuestions; i++) {
    questions.push({
      id: `q${i + 1}`,
      header: `Question ${i + 1}`,
      question: `What is your answer for question ${i + 1}?`,
      multiSelect: i % 2 === 1, // Odd questions are multi-select
      options: [
        { index: 1, label: 'Option A', description: 'First option' },
        { index: 2, label: 'Option B', description: 'Second option' },
        { index: 3, label: 'Option C' },
      ],
    });
  }
  return {
    id: 'test-prompt',
    questions,
  };
}

// Helper to set up a session with a prompt
function setupPromptSession(sessionId: string, numQuestions: number = 2): InteractivePrompt {
  const prompt = createTestPrompt(numQuestions);
  const event: InteractiveEvent = {
    type: 'prompt_presented',
    session_id: sessionId,
    prompt,
  };
  handleInteractiveEvent(event);
  return prompt;
}

describe('Interactive Store', () => {
  beforeEach(() => {
    // Reset the store by setting all sessions to idle
    const states = interactiveStates();
    for (const sessionId of states.keys()) {
      clearInteractiveState(sessionId);
    }
  });

  describe('getInteractiveState', () => {
    it('returns idle for unknown session', () => {
      const state = getInteractiveState('unknown-session');
      expect(state.type).toBe('idle');
    });

    it('returns prompt state after event', () => {
      setupPromptSession('session-1');
      const state = getInteractiveState('session-1');
      expect(state.type).toBe('prompt');
    });
  });

  describe('handleInteractiveEvent', () => {
    describe('prompt_presented', () => {
      it('creates a prompt session', () => {
        const prompt = setupPromptSession('session-1');
        const state = getInteractiveState('session-1');

        expect(state.type).toBe('prompt');
        if (state.type === 'prompt') {
          expect(state.session.prompt.id).toBe(prompt.id);
          expect(state.session.currentIndex).toBe(0);
          expect(state.session.answers.size).toBe(0);
        }
      });

      it('converts snake_case to camelCase', () => {
        // Simulate backend sending snake_case
        const event = {
          type: 'prompt_presented' as const,
          session_id: 'session-1',
          prompt: {
            id: 'test',
            questions: [
              {
                id: 'q1',
                header: 'Test',
                question: 'Test?',
                multi_select: true, // snake_case from backend
                options: [{ index: 1, label: 'A' }],
              },
            ],
          },
        } as unknown as InteractiveEvent; // Cast through unknown to test snake_case conversion
        handleInteractiveEvent(event);

        const state = getInteractiveState('session-1');
        if (state.type === 'prompt') {
          expect(state.session.prompt.questions[0].multiSelect).toBe(true);
        }
      });

      it('handles questions without multi_select field', () => {
        const event = {
          type: 'prompt_presented' as const,
          session_id: 'session-1',
          prompt: {
            id: 'test',
            questions: [
              {
                id: 'q1',
                header: 'Test',
                question: 'Test?',
                // No multi_select field
                options: [{ index: 1, label: 'A' }],
              },
            ],
          },
        } as unknown as InteractiveEvent; // Cast through unknown to test missing multi_select
        handleInteractiveEvent(event);

        const state = getInteractiveState('session-1');
        if (state.type === 'prompt') {
          expect(state.session.prompt.questions[0].multiSelect).toBe(false);
        }
      });

      it('stores question options correctly', () => {
        setupPromptSession('session-1');
        const state = getInteractiveState('session-1');

        if (state.type === 'prompt') {
          const question = state.session.prompt.questions[0];
          expect(question.options.length).toBe(3);
          expect(question.options[0].index).toBe(1);
          expect(question.options[0].label).toBe('Option A');
          expect(question.options[0].description).toBe('First option');
          expect(question.options[2].description).toBeUndefined();
        }
      });
    });

    describe('interaction_complete', () => {
      it('resets session to idle', () => {
        setupPromptSession('session-1');
        expect(getInteractiveState('session-1').type).toBe('prompt');

        handleInteractiveEvent({
          type: 'interaction_complete',
          session_id: 'session-1',
        });

        expect(getInteractiveState('session-1').type).toBe('idle');
      });

      it('handles completion for idle session gracefully', () => {
        handleInteractiveEvent({
          type: 'interaction_complete',
          session_id: 'never-existed',
        });

        expect(getInteractiveState('never-existed').type).toBe('idle');
      });
    });
  });

  describe('setQuestionAnswer', () => {
    it('sets answer for a question', () => {
      setupPromptSession('session-1');
      setQuestionAnswer('session-1', 'q1', [1]);

      const state = getInteractiveState('session-1');
      if (state.type === 'prompt') {
        expect(state.session.answers.get('q1')).toEqual([1]);
      }
    });

    it('supports multiple selected indices for multi-select', () => {
      setupPromptSession('session-1');
      setQuestionAnswer('session-1', 'q2', [1, 2, 3]);

      const state = getInteractiveState('session-1');
      if (state.type === 'prompt') {
        expect(state.session.answers.get('q2')).toEqual([1, 2, 3]);
      }
    });

    it('overwrites previous answer', () => {
      setupPromptSession('session-1');
      setQuestionAnswer('session-1', 'q1', [1]);
      setQuestionAnswer('session-1', 'q1', [2]);

      const state = getInteractiveState('session-1');
      if (state.type === 'prompt') {
        expect(state.session.answers.get('q1')).toEqual([2]);
      }
    });

    it('does nothing for idle session', () => {
      setQuestionAnswer('no-session', 'q1', [1]);
      expect(getInteractiveState('no-session').type).toBe('idle');
    });

    it('preserves other answers when setting one', () => {
      setupPromptSession('session-1');
      setQuestionAnswer('session-1', 'q1', [1]);
      setQuestionAnswer('session-1', 'q2', [2]);

      const state = getInteractiveState('session-1');
      if (state.type === 'prompt') {
        expect(state.session.answers.get('q1')).toEqual([1]);
        expect(state.session.answers.get('q2')).toEqual([2]);
      }
    });
  });

  describe('setCurrentQuestionIndex', () => {
    it('sets current index', () => {
      setupPromptSession('session-1', 5);
      setCurrentQuestionIndex('session-1', 2);

      const state = getInteractiveState('session-1');
      if (state.type === 'prompt') {
        expect(state.session.currentIndex).toBe(2);
      }
    });

    it('clamps to minimum of 0', () => {
      setupPromptSession('session-1', 3);
      setCurrentQuestionIndex('session-1', -5);

      const state = getInteractiveState('session-1');
      if (state.type === 'prompt') {
        expect(state.session.currentIndex).toBe(0);
      }
    });

    it('clamps to maximum (questions.length - 1)', () => {
      setupPromptSession('session-1', 3);
      setCurrentQuestionIndex('session-1', 100);

      const state = getInteractiveState('session-1');
      if (state.type === 'prompt') {
        expect(state.session.currentIndex).toBe(2);
      }
    });

    it('does nothing for idle session', () => {
      setCurrentQuestionIndex('no-session', 5);
      expect(getInteractiveState('no-session').type).toBe('idle');
    });
  });

  describe('goToNextQuestion', () => {
    it('increments current index', () => {
      setupPromptSession('session-1', 5);
      expect(getInteractiveState('session-1').type).toBe('prompt');

      goToNextQuestion('session-1');

      const state = getInteractiveState('session-1');
      if (state.type === 'prompt') {
        expect(state.session.currentIndex).toBe(1);
      }
    });

    it('does not exceed maximum index', () => {
      setupPromptSession('session-1', 3);
      setCurrentQuestionIndex('session-1', 2); // Last question

      goToNextQuestion('session-1');

      const state = getInteractiveState('session-1');
      if (state.type === 'prompt') {
        expect(state.session.currentIndex).toBe(2); // Still at last
      }
    });

    it('does nothing for idle session', () => {
      goToNextQuestion('no-session');
      expect(getInteractiveState('no-session').type).toBe('idle');
    });
  });

  describe('goToPreviousQuestion', () => {
    it('decrements current index', () => {
      setupPromptSession('session-1', 5);
      setCurrentQuestionIndex('session-1', 3);

      goToPreviousQuestion('session-1');

      const state = getInteractiveState('session-1');
      if (state.type === 'prompt') {
        expect(state.session.currentIndex).toBe(2);
      }
    });

    it('does not go below 0', () => {
      setupPromptSession('session-1', 3);
      // Already at index 0

      goToPreviousQuestion('session-1');

      const state = getInteractiveState('session-1');
      if (state.type === 'prompt') {
        expect(state.session.currentIndex).toBe(0);
      }
    });

    it('does nothing for idle session', () => {
      goToPreviousQuestion('no-session');
      expect(getInteractiveState('no-session').type).toBe('idle');
    });
  });

  describe('getAnsweredCount', () => {
    it('returns 0 for idle session', () => {
      expect(getAnsweredCount('no-session')).toBe(0);
    });

    it('returns 0 for new prompt session', () => {
      setupPromptSession('session-1');
      expect(getAnsweredCount('session-1')).toBe(0);
    });

    it('returns correct count after answers', () => {
      setupPromptSession('session-1', 5);
      setQuestionAnswer('session-1', 'q1', [1]);
      setQuestionAnswer('session-1', 'q3', [2]);

      expect(getAnsweredCount('session-1')).toBe(2);
    });

    it('does not double count re-answered questions', () => {
      setupPromptSession('session-1');
      setQuestionAnswer('session-1', 'q1', [1]);
      setQuestionAnswer('session-1', 'q1', [2]);

      expect(getAnsweredCount('session-1')).toBe(1);
    });
  });

  describe('allQuestionsAnswered', () => {
    it('returns false for idle session', () => {
      expect(allQuestionsAnswered('no-session')).toBe(false);
    });

    it('returns false for new prompt', () => {
      setupPromptSession('session-1', 2);
      expect(allQuestionsAnswered('session-1')).toBe(false);
    });

    it('returns false when partially answered', () => {
      setupPromptSession('session-1', 3);
      setQuestionAnswer('session-1', 'q1', [1]);
      setQuestionAnswer('session-1', 'q2', [2]);

      expect(allQuestionsAnswered('session-1')).toBe(false);
    });

    it('returns true when all answered', () => {
      setupPromptSession('session-1', 2);
      setQuestionAnswer('session-1', 'q1', [1]);
      setQuestionAnswer('session-1', 'q2', [2]);

      expect(allQuestionsAnswered('session-1')).toBe(true);
    });

    it('returns true for single question prompt when answered', () => {
      setupPromptSession('session-1', 1);
      setQuestionAnswer('session-1', 'q1', [1]);

      expect(allQuestionsAnswered('session-1')).toBe(true);
    });
  });

  describe('getOrderedAnswers', () => {
    it('returns empty array for idle session', () => {
      expect(getOrderedAnswers('no-session')).toEqual([]);
    });

    it('returns answers in question order', () => {
      setupPromptSession('session-1', 3);
      // Answer out of order
      setQuestionAnswer('session-1', 'q3', [3]);
      setQuestionAnswer('session-1', 'q1', [1]);
      setQuestionAnswer('session-1', 'q2', [2]);

      const answers = getOrderedAnswers('session-1');
      expect(answers).toEqual([
        { questionId: 'q1', selectedIndices: [1] },
        { questionId: 'q2', selectedIndices: [2] },
        { questionId: 'q3', selectedIndices: [3] },
      ]);
    });

    it('returns empty indices for unanswered questions', () => {
      setupPromptSession('session-1', 3);
      setQuestionAnswer('session-1', 'q2', [2]);

      const answers = getOrderedAnswers('session-1');
      expect(answers[0].selectedIndices).toEqual([]);
      expect(answers[1].selectedIndices).toEqual([2]);
      expect(answers[2].selectedIndices).toEqual([]);
    });
  });

  describe('clearInteractiveState', () => {
    it('sets session to idle', () => {
      setupPromptSession('session-1');
      expect(getInteractiveState('session-1').type).toBe('prompt');

      clearInteractiveState('session-1');
      expect(getInteractiveState('session-1').type).toBe('idle');
    });

    it('handles already idle session', () => {
      clearInteractiveState('never-existed');
      expect(getInteractiveState('never-existed').type).toBe('idle');
    });

    it('clears answers when clearing state', () => {
      setupPromptSession('session-1');
      setQuestionAnswer('session-1', 'q1', [1]);
      expect(getAnsweredCount('session-1')).toBe(1);

      clearInteractiveState('session-1');
      expect(getAnsweredCount('session-1')).toBe(0);
    });
  });

  describe('hasActivePrompt', () => {
    it('returns false for idle session', () => {
      expect(hasActivePrompt('no-session')).toBe(false);
    });

    it('returns true for prompt session', () => {
      setupPromptSession('session-1');
      expect(hasActivePrompt('session-1')).toBe(true);
    });

    it('returns false after clearing', () => {
      setupPromptSession('session-1');
      clearInteractiveState('session-1');
      expect(hasActivePrompt('session-1')).toBe(false);
    });

    it('returns false after interaction_complete', () => {
      setupPromptSession('session-1');
      handleInteractiveEvent({
        type: 'interaction_complete',
        session_id: 'session-1',
      });
      expect(hasActivePrompt('session-1')).toBe(false);
    });
  });

  describe('Multiple Sessions', () => {
    it('maintains independent state for different sessions', () => {
      setupPromptSession('session-1', 2);
      setupPromptSession('session-2', 3);

      setQuestionAnswer('session-1', 'q1', [1]);
      setCurrentQuestionIndex('session-2', 2);

      const state1 = getInteractiveState('session-1');
      const state2 = getInteractiveState('session-2');

      if (state1.type === 'prompt' && state2.type === 'prompt') {
        expect(state1.session.answers.size).toBe(1);
        expect(state2.session.answers.size).toBe(0);
        expect(state1.session.currentIndex).toBe(0);
        expect(state2.session.currentIndex).toBe(2);
        expect(state1.session.prompt.questions.length).toBe(2);
        expect(state2.session.prompt.questions.length).toBe(3);
      }
    });

    it('clearing one session does not affect others', () => {
      setupPromptSession('session-1');
      setupPromptSession('session-2');

      clearInteractiveState('session-1');

      expect(hasActivePrompt('session-1')).toBe(false);
      expect(hasActivePrompt('session-2')).toBe(true);
    });
  });

  describe('Edge Cases', () => {
    it('handles empty questions array', () => {
      const event: InteractiveEvent = {
        type: 'prompt_presented',
        session_id: 'session-1',
        prompt: { id: 'empty', questions: [] },
      };
      handleInteractiveEvent(event);

      const state = getInteractiveState('session-1');
      if (state.type === 'prompt') {
        expect(state.session.prompt.questions.length).toBe(0);
      }
      expect(allQuestionsAnswered('session-1')).toBe(true); // 0 == 0
    });

    it('handles setting index on empty questions array', () => {
      const event: InteractiveEvent = {
        type: 'prompt_presented',
        session_id: 'session-1',
        prompt: { id: 'empty', questions: [] },
      };
      handleInteractiveEvent(event);

      // Should clamp to valid range (which is nothing for empty array)
      setCurrentQuestionIndex('session-1', 5);

      const state = getInteractiveState('session-1');
      if (state.type === 'prompt') {
        // Math.max(0, Math.min(5, -1)) = Math.max(0, -1) = 0
        expect(state.session.currentIndex).toBe(0);
      }
    });

    it('handles empty selectedIndices', () => {
      setupPromptSession('session-1');
      setQuestionAnswer('session-1', 'q1', []);

      const state = getInteractiveState('session-1');
      if (state.type === 'prompt') {
        expect(state.session.answers.get('q1')).toEqual([]);
        expect(state.session.answers.size).toBe(1); // Still counts as answered
      }
    });
  });
});
