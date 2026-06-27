import { create } from 'zustand';
import type { LoginCredentials, LoginMetadata } from '../generated/auth-contracts';
import { fetchLoginMetadata, postLogin, postLogout, startRedirectLogin } from '../api/auth';
import { setStoredToken, clearStoredToken } from '../api/client';
import { closeAllStreamConnections } from '../api/streamRegistry';
import { useCurrentUserStore } from './currentUserStore';
import { useEventStore } from './eventStore';

interface AuthState {
  metadata: LoginMetadata | null;
  isMetadataLoading: boolean;

  isLoginLoading: boolean;
  loginError: string | null;

  fetchMetadata: () => Promise<LoginMetadata | null>;
  login: (credentials: LoginCredentials) => Promise<boolean>;
  startRedirectLogin: () => Promise<void>;
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

  startRedirectLogin: async () => {
    set({ isLoginLoading: true, loginError: null });
    try {
      const response = await startRedirectLogin({
        return_to: window.location.href,
      });
      window.location.assign(response.auth_url);
    } catch (e) {
      set({
        isLoginLoading: false,
        loginError: (e as Error).message || '启动登录失败',
      });
    }
  },

  logout: () => {
    void postLogout().catch((err: unknown) => {
      console.warn('logout: 后端撤销 token 失败', err);
    });
    closeAllStreamConnections();
    useEventStore.getState().disconnect();
    useCurrentUserStore.getState().clear();
    clearStoredToken();
    set({ loginError: null });
  },
}));
