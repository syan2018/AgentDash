import { expect, test } from "@playwright/test";

test("Task 创建与详情编辑使用统一 agent_binding 结构", async ({ page }) => {
  const suffix = Date.now().toString();
  const projectName = `E2E 项目 ${suffix}`;
  const storyTitle = `E2E Story ${suffix}`;
  const taskTitle = `E2E Task ${suffix}`;

  await page.goto("/");

  await page.getByRole("button", { name: "+ 新建" }).first().click();
  await expect(page.getByRole("heading", { name: "新建项目" })).toBeVisible();
  await page.getByPlaceholder("项目名称").fill(projectName);
  await page.getByRole("button", { name: "创建项目" }).click();

  await expect(page.getByRole("heading", { name: "Story 列表" })).toBeVisible();

  await page.getByRole("button", { name: "+ 创建" }).click();
  await page.getByPlaceholder("Story 标题").fill(storyTitle);
  await page.getByRole("button", { name: "创建 Story" }).click();

  const storyCard = page.locator("button").filter({ hasText: storyTitle }).first();
  await expect(storyCard).toBeVisible();
  await storyCard.click();
  await expect(page).toHaveURL(/\/story\//);

  await page.getByRole("button", { name: "添加 Task" }).click();
  await page.getByPlaceholder("Task 标题").fill(taskTitle);

  const createAgentTypeSelect = page
    .locator("label:has-text('Agent 类型')")
    .locator("xpath=following-sibling::select")
    .first();
  await expect.poll(async () => createAgentTypeSelect.locator("option").count()).toBeGreaterThan(1);
  await createAgentTypeSelect.selectOption({ index: 1 });
  const selectedAgentType = await createAgentTypeSelect.inputValue();
  expect(selectedAgentType).not.toBe("");

  const createPromptTemplate = page
    .locator("label:has-text('Prompt 模板')")
    .locator("xpath=following-sibling::textarea")
    .first();
  const createInitialContext = page
    .locator("label:has-text('Initial Context')")
    .locator("xpath=following-sibling::textarea")
    .first();

  await createPromptTemplate.fill("请完成自动化测试任务");
  await createInitialContext.fill("这是创建阶段写入的上下文");
  await page.getByRole("button", { name: "创建" }).click();

  await page.getByRole("button", { name: "任务列表" }).click();
  const taskCard = page.locator("button").filter({ hasText: taskTitle }).first();
  await expect(taskCard).toBeVisible();
  await taskCard.click();

  const drawerAgentTypeSelect = page
    .locator("label:has-text('Agent 类型')")
    .locator("xpath=following-sibling::select")
    .last();
  const drawerPromptTemplate = page
    .locator("label:has-text('Prompt 模板')")
    .locator("xpath=following-sibling::textarea")
    .last();
  const drawerInitialContext = page
    .locator("label:has-text('Initial Context')")
    .locator("xpath=following-sibling::textarea")
    .last();

  await expect(drawerAgentTypeSelect).toHaveValue(selectedAgentType);
  await expect(drawerPromptTemplate).toHaveValue("请完成自动化测试任务");
  await expect(drawerInitialContext).toHaveValue("这是创建阶段写入的上下文");

  await drawerPromptTemplate.fill("更新后的模板");
  await drawerInitialContext.fill("更新后的上下文");
  await page.getByRole("button", { name: "保存 Task" }).click();

  await page.getByRole("button", { name: "关闭" }).last().click();
  await taskCard.click();
  await expect(drawerPromptTemplate).toHaveValue("更新后的模板");
  await expect(drawerInitialContext).toHaveValue("更新后的上下文");
});
