import { api } from './client';
import type { LoginCredentials, LoginMetadata, LoginResponse } from '../types';

export async function fetchLoginMetadata(): Promise<LoginMetadata> {
  return api.get<LoginMetadata>('/auth/metadata');
}

export async function postLogin(credentials: LoginCredentials): Promise<LoginResponse> {
  return api.post<LoginResponse>('/auth/login', credentials);
}
