import { api } from "../api/client";
import type { DirectoryGroup, DirectoryUser } from "../types";

export async function fetchDirectoryUsers(): Promise<DirectoryUser[]> {
  return api.get<DirectoryUser[]>("/directory/users");
}

export async function fetchDirectoryGroups(): Promise<DirectoryGroup[]> {
  return api.get<DirectoryGroup[]>("/directory/groups");
}
