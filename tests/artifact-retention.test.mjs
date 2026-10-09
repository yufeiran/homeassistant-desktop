import assert from 'node:assert/strict';
import test from 'node:test';
import retainLatest from '../.github/scripts/retain-latest-build.cjs';

const names = ['Windows-x64', 'Windows-ARM64', 'macOS-x64', 'macOS-ARM64', 'Linux-x64', 'Linux-ARM64'];
const run = (id, overrides = {}) => ({
  id, workflow_id: 7, status: 'completed', conclusion: 'success', event: 'push',
  created_at: `2026-10-${String(id).padStart(2, '0')}T00:00:00Z`,
  head_repository: { full_name: 'owner/repo' }, ...overrides,
});
const artifacts = id => names.map((platform, index) => ({
  id: id * 100 + index, name: `homeassistant-desktop-${platform}`,
  expired: false, size_in_bytes: 1024, workflow_run: { id },
}));

async function execute({ runs = [run(1), run(2)], items = [...artifacts(1), ...artifacts(2)],
  trigger = runs.at(-1), refresh = {}, refreshedArtifacts = {}, deletionError } = {}) {
  const deleted = [];
  const calls = new Map();
  const api = {
    listWorkflowRunArtifacts: Symbol('run artifacts'),
    listArtifactsForRepo: Symbol('repository artifacts'),
    getWorkflowRun: async ({ run_id }) => {
      const count = (calls.get(run_id) ?? 0) + 1;
      calls.set(run_id, count);
      return { data: count > 1 && refresh[run_id] ? refresh[run_id] : runs.find(r => r.id === run_id) };
    },
    deleteArtifact: async ({ artifact_id }) => {
      if (deletionError) throw deletionError;
      deleted.push(artifact_id);
    },
  };
  const artifactReads = new Map();
  const github = {
    rest: { actions: api },
    paginate: async (method, params) => {
      assert.equal(params.per_page, 100);
      if (method === api.listArtifactsForRepo) return items;
      assert.equal(method, api.listWorkflowRunArtifacts);
      const count = (artifactReads.get(params.run_id) ?? 0) + 1;
      artifactReads.set(params.run_id, count);
      return count > 1 && refreshedArtifacts[params.run_id] ? refreshedArtifacts[params.run_id] :
        items.filter(a => a.workflow_run.id === params.run_id);
    },
  };
  await retainLatest({ github, core: { info() {}, warning() {} },
    context: { repo: { owner: 'owner', repo: 'repo' }, payload: { workflow_run: trigger } } });
  return deleted;
}

test('retains the complete latest build and removes every older platform package', async () => {
  assert.deepEqual(await execute(), artifacts(1).map(a => a.id));
});

test('a failed build or fork event cannot delete the previous successful build', async () => {
  assert.deepEqual(await execute({ trigger: run(2, { conclusion: 'failure' }) }), []);
  assert.deepEqual(await execute({ trigger: run(2, { head_repository: { full_name: 'fork/repo' } }) }), []);
});

test('missing, duplicate, empty or expired latest platform artifacts prevent cleanup', async () => {
  for (const latest of [artifacts(2).slice(1), [...artifacts(2), artifacts(2)[0]],
    artifacts(2).map((a, i) => i === 0 ? { ...a, size_in_bytes: 0 } : a),
    artifacts(2).map((a, i) => i === 0 ? { ...a, expired: true } : a)]) {
    assert.deepEqual(await execute({ items: [...artifacts(1), ...latest] }), []);
  }
});

test('out-of-order completion still keeps the newer build', async () => {
  assert.deepEqual(await execute({ trigger: run(1) }), artifacts(1).map(a => a.id));
});

test('preserves concurrent builds, unrelated artifacts and other workflows', async () => {
  const unrelated = { ...artifacts(1)[0], id: 999, name: 'test-report' };
  assert.deepEqual(await execute({ runs: [run(1, { status: 'in_progress', conclusion: null }), run(2)],
    items: [...artifacts(1), ...artifacts(2), unrelated] }), []);
  assert.deepEqual(await execute({ runs: [run(1, { workflow_id: 8 }), run(2)] }), []);
  assert.deepEqual(await execute({ items: [...artifacts(1), ...artifacts(2), unrelated] }), artifacts(1).map(a => a.id));
});

test('rechecks runs and retained artifacts before deleting', async () => {
  assert.deepEqual(await execute({ refresh: { 1: run(1, { status: 'in_progress' }) } }), []);
  assert.deepEqual(await execute({ refresh: { 2: run(2, { status: 'in_progress' }) } }), []);
  assert.deepEqual(await execute({ refreshedArtifacts: { 2: artifacts(2).slice(1) } }), []);
});

test('newer incomplete builds are left intact and do not replace the latest good build', async () => {
  const runs = [run(1), run(2), run(3, { conclusion: 'failure' })];
  const items = [...artifacts(1), ...artifacts(2), ...artifacts(3).slice(1)];
  assert.deepEqual(await execute({ runs, items, trigger: run(2) }), artifacts(1).map(a => a.id));
});

test('tolerates artifacts already removed but reports permission failures', async () => {
  assert.deepEqual(await execute({ deletionError: { status: 404 } }), []);
  await assert.rejects(execute({ deletionError: Object.assign(new Error('Forbidden'), { status: 403 }) }), /Forbidden/);
});
