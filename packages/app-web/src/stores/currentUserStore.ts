import { create } from "zustand";
import type { CurrentUser } from "../types";
import { fetchCurrentUser } from "../services/currentUser";

interface CurrentUserState {
  currentUser: CurrentUser | null;
  isLoading: boolean;
  hasLoaded: boolean;
  error: string | null;

  fetchCurrentUser: () => Promise<CurrentUser | null>;
  clear: () => void;
}

export const useCurrentUserStore = create<CurrentUserState>((set) => ({
  currentUser: null,
  isLoading: false,
  hasLoaded: false,
  error: null,

  fetchCurrentUser: async () => {
    set({ isLoading: true, error: null });
    try {
      const currentUser = await fetchCurrentUser();
      set({
        currentUser,
        isLoading: false,
        hasLoaded: true,
        error: null,
      });
      return currentUser;
    } catch (e) {
      set({
        currentUser: null,
        isLoading: false,
        hasLoaded: true,
        error: (e as Error).message,
      });
      return null;
    }
  },

  clear: () => set({
    currentUser: null,
    isLoading: false,
    hasLoaded: false,
    error: null,
  }),
}));
