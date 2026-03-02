import { expect, test } from "@playwright/test";

test("仪表盘首屏可用", async ({ page }) => {
  await page.goto("/");
  await expect(page.getByText("请选择或创建项目")).toBeVisible();
});

test("会话页可打开", async ({ page }) => {
  await page.goto("/session");
  await expect(page.getByRole("heading", { name: "会话" })).toBeVisible();
});
