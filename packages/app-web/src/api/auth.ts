import { api } from './client';
import type {
  AuthStartRequest,
  AuthStartResponse,
  LoginCredentials,
  LoginMetadata,
  LoginResponse,
} from '../types';

export async function fetchLoginMetadata(): Promise<LoginMetadata> {
  return api.get<LoginMetadata>('/auth/metadata');
}

export async function postLogin(credentials: LoginCredentials): Promise<LoginResponse> {
  return api.post<LoginResponse>('/auth/login', credentials);
}

export async function startRedirectLogin(request: AuthStartRequest): Promise<AuthStartResponse> {
  return api.post<AuthStartResponse>('/auth/oidc/start', request);
}
