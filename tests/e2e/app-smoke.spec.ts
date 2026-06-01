import { expect, test } from "@playwright/test";

test("仪表盘首屏可用", async ({ page }) => {
  await page.goto("/");
  await expect(page.getByText("请选择或创建项目")).toBeVisible();
});

test("未知会话入口回到仪表盘", async ({ page }) => {
  await page.goto("/session");
  await expect(page).toHaveURL(/\/dashboard\/agent$/);
});
