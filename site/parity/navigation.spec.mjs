import {test,expect} from '@playwright/test';
test('project navigation, counted views and selection match the app',async({page})=>{
 await page.goto('/');
 await expect(page.locator('[data-task]').first()).toContainText('▸ ■ T12');
 await expect(page.locator('[data-task]').first()).toHaveCSS('background-color','rgba(0, 0, 0, 0)');
 await expect(page.locator('.tsk-tab-group.is-on')).toHaveCSS('text-decoration-line','underline');
 await page.locator('[data-tab="project"]').click();
 await expect(page.locator('.tsk-project-selector')).toHaveCount(0);
 await expect(page.locator('[data-filter]')).toHaveText('all ▾');
 await page.locator('[data-filter]').click();
 await expect(page.getByRole('dialog',{name:'thread filter'})).toBeVisible();
 await page.getByText('Without a thread 1',{exact:false}).click();
 await expect(page.locator('[data-task]')).toHaveCount(1);
 await page.locator('[data-tab="projects"]').click();
 await expect(page.locator('.tsk-project-legend')).toContainText('NEEDS YOU');
 await expect(page.locator('[data-filter]')).toHaveText('Overview ▾');
 await page.locator('[data-project-row]').first().click();
 await expect(page.locator('.tsk-project-legend')).toBeVisible();
 await page.locator('[data-filter]').click();
 await page.getByText('#release 2',{exact:false}).click();
 await expect(page.locator('[data-task]')).toHaveCount(2);
 await page.locator('[data-chip]').click();
 await expect(page.getByRole('dialog',{name:'project',exact:true})).toContainText('archived (0)');
});

test('full terminal opens with board and task side by side', async ({page}) => {
 await page.route('http://127.0.0.1:4178/', async route => {
  const response = await route.fetch();
  const html = (await response.text())
   .replace('width:78ch', 'width:130ch')
   .replace('<div id="tsk-demo"', '<button data-layout="full" aria-pressed="true">full terminal</button><div id="tsk-demo"');
  await route.fulfill({response, body:html});
 });
 await page.goto('/');
 await expect(page.locator('.tsk-wide-split.is-split')).toBeVisible();
 await expect(page.locator('.tsk-board-surface [data-task]').first()).toBeVisible();
 await expect(page.locator('.tsk-task-header')).toBeVisible();
});
