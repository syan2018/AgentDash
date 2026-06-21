import { api } from "../api/client";
import type { CurrentUser } from "../generated/auth-contracts";

export async function fetchCurrentUser(): Promise<CurrentUser> {
  return api.get<CurrentUser>("/me");
}
