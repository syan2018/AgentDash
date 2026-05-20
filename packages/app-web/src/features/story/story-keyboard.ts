import { useEffect } from "react";
import { useNavigate } from "react-router-dom";
import { useStoryStore } from "../../stores/storyStore";
import { useStoryViewStore } from "../../stores/storyViewStore";

function isInEditableTarget(target: EventTarget | null): boolean {
  if (!(target instanceof HTMLElement)) return false;
  const tag = target.tagName;
  if (tag === "INPUT" || tag === "TEXTAREA" || tag === "SELECT") return true;
  if (target.isContentEditable) return true;
  return false;
}

function isModKey(event: KeyboardEvent): boolean {
  return event.metaKey || event.ctrlKey;
}

interface UseStoryHotkeysOptions {
  scope: "tab" | "page";
}

export function useStoryHotkeys({ scope }: UseStoryHotkeysOptions) {
  const navigate = useNavigate();

  useEffect(() => {
    const handler = (event: KeyboardEvent) => {
      if (isInEditableTarget(event.target)) return;

      const view = useStoryViewStore.getState();

      if (event.key === "Escape") {
        if (view.isQuickJumpOpen) {
          event.preventDefault();
          view.setQuickJumpOpen(false);
          return;
        }
        if (view.selectedIds.size > 0) {
          event.preventDefault();
          view.clearSelection();
          return;
        }
      }

      if (isModKey(event) && (event.key === "k" || event.key === "K")) {
        event.preventDefault();
        view.setQuickJumpOpen(true);
        return;
      }

      if (scope === "tab" && isModKey(event) && (event.key === "n" || event.key === "N")) {
        event.preventDefault();
        view.openCreate();
        return;
      }

      if (scope === "tab" && view.focusedStoryId) {
        const focusedId = view.focusedStoryId;
        if (event.key === "e" || event.key === "E") {
          event.preventDefault();
          navigate(`/story/${focusedId}`);
          return;
        }
        if (event.key === "p" || event.key === "P") {
          event.preventDefault();
          view.requestPicker(focusedId, "priority");
          return;
        }
        if (event.key === "x" || event.key === "X") {
          event.preventDefault();
          const allStories = Object.values(useStoryStore.getState().storiesByProjectId).flat();
          const story = allStories.find((s) => s.id === focusedId);
          if (story) {
            const target = story.status === "completed" ? "ready" : "completed";
            void useStoryStore.getState().updateStory(focusedId, { status: target });
          }
          return;
        }
      }
    };

    window.addEventListener("keydown", handler, true);
    return () => window.removeEventListener("keydown", handler, true);
  }, [navigate, scope]);
}
