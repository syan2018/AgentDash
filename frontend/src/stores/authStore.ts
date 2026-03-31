import { create } from 'zustand';
import type { LoginCredentials, LoginMetadata } from '../types';
import { fetchLoginMetadata, postLogin } from '../api/auth';
import { setStoredToken, clearStoredToken } from '../api/client';
import { useCurrentUserStore } from './currentUserStore';

interface AuthState {
  metadata: LoginMetadata | null;
  isMetadataLoading: boolean;

  isLoginLoading: boolean;
  loginError: string | null;

  fetchMetadata: () => Promise<LoginMetadata | null>;
  login: (credentials: LoginCredentials) => Promise<boolean>;
  logout: () => void;
}

export const useAuthStore = create<AuthState>((set) => ({
  metadata: null,
  isMetadataLoading: false,

  isLoginLoading: false,
  loginError: null,

  fetchMetadata: async () => {
    set({ isMetadataLoading: true });
    try {
      const metadata = await fetchLoginMetadata();
      set({ metadata, isMetadataLoading: false });
      return metadata;
    } catch {
      set({ metadata: null, isMetadataLoading: false });
      return null;
    }
  },

  login: async (credentials: LoginCredentials) => {
    set({ isLoginLoading: true, loginError: null });
    try {
      const response = await postLogin(credentials);
      setStoredToken(response.access_token);

      await useCurrentUserStore.getState().fetchCurrentUser();

      set({ isLoginLoading: false, loginError: null });
      return true;
    } catch (e) {
      set({
        isLoginLoading: false,
        loginError: (e as Error).message || '登录失败',
      });
      return false;
    }
  },

  logout: () => {
    clearStoredToken();
    useCurrentUserStore.getState().clear();
    set({ loginError: null });
  },
}));
