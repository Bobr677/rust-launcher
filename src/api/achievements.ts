import { apiFetch } from "./client";
import { ensureValidAccessToken } from "./auth";

export type AchievementRow = {
  code: string;
  title: string;
  description: string;
  icon_url?: string | null;
  unlocked: boolean;
  unlocked_at?: string | null;
};

export async function listAchievements(): Promise<AchievementRow[]> {
  await ensureValidAccessToken();
  const data = await apiFetch<{ achievements: AchievementRow[] }>("/achievements");
  return data.achievements ?? [];
}

export async function listUserAchievements(userId: string): Promise<AchievementRow[]> {
  await ensureValidAccessToken();
  const data = await apiFetch<{ achievements: AchievementRow[] }>(`/users/${userId}/achievements`);
  return data.achievements ?? [];
}
