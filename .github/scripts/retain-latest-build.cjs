const artifactNames = new Set([
  'Windows-x64', 'Windows-ARM64', 'macOS-x64', 'macOS-ARM64',
  'Linux-x64', 'Linux-ARM64',
].map(platform => `homeassistant-desktop-${platform}`));

function complete(artifacts) {
  return artifacts.length === artifactNames.size &&
    new Set(artifacts.map(artifact => artifact.name)).size === artifactNames.size &&
    artifacts.every(artifact => artifactNames.has(artifact.name) &&
      !artifact.expired && artifact.size_in_bytes > 0);
}

function order(run) {
  const time = Date.parse(run.created_at);
  if (!Number.isFinite(time)) throw new Error(`Invalid date for run ${run.id}`);
  return time;
}

function older(a, b) {
  return order(a) < order(b) || (order(a) === order(b) && a.id < b.id);
}

module.exports = async function retainLatest({ github, core, context }) {
  const repo = context.repo;
  const repository = `${repo.owner}/${repo.repo}`;
  const trigger = context.payload.workflow_run;
  if (trigger?.conclusion !== 'success' || trigger.event !== 'push' ||
      trigger.head_repository?.full_name !== repository) {
    core.info('No successful build from this repository; keeping existing artifacts.');
    return;
  }

  const runArtifacts = run_id => github.paginate(
    github.rest.actions.listWorkflowRunArtifacts, { ...repo, run_id, per_page: 100 },
  );
  const uploaded = (await runArtifacts(trigger.id)).filter(a => artifactNames.has(a.name));
  if (!complete(uploaded)) {
    core.warning('Build does not have all six valid platform artifacts; skipping cleanup.');
    return;
  }

  const artifacts = (await github.paginate(
    github.rest.actions.listArtifactsForRepo, { ...repo, per_page: 100 },
  )).filter(a => artifactNames.has(a.name));
  const groups = new Map();
  for (const artifact of artifacts) {
    const id = artifact.workflow_run?.id;
    if (!id) throw new Error(`Artifact ${artifact.id} has no workflow run ID`);
    if (!groups.has(id)) groups.set(id, []);
    groups.get(id).push(artifact);
  }

  const runs = new Map();
  for (const id of groups.keys()) {
    const { data: run } = await github.rest.actions.getWorkflowRun({ ...repo, run_id: id });
    // Ignore artifacts belonging to another workflow or a fork.
    if (run.workflow_id === trigger.workflow_id && run.event === 'push' &&
        run.head_repository?.full_name === repository) {
      order(run);
      runs.set(id, run);
    }
  }

  const candidates = [...runs.values()].filter(run => run.status === 'completed' &&
    run.conclusion === 'success' && complete(groups.get(run.id)));
  const latest = candidates.reduce((newest, run) =>
    !newest || older(newest, run) ? run : newest, null);
  if (!latest) {
    core.warning('No complete successful build is available; skipping cleanup.');
    return;
  }

  // Recheck the retained build before making any destructive API calls.
  const { data: retainedRun } = await github.rest.actions.getWorkflowRun({
    ...repo, run_id: latest.id,
  });
  const retained = (await runArtifacts(latest.id)).filter(a => artifactNames.has(a.name));
  const retainedIds = new Set(groups.get(latest.id).map(a => a.id));
  if (retainedRun.status !== 'completed' || retainedRun.conclusion !== 'success' ||
      !complete(retained) || retained.some(a => !retainedIds.has(a.id))) {
    core.warning('The retained build changed during cleanup; keeping existing artifacts.');
    return;
  }

  let deleted = 0;
  for (const run of runs.values()) {
    if (!older(run, latest) || run.status !== 'completed') continue;
    const { data: fresh } = await github.rest.actions.getWorkflowRun({
      ...repo, run_id: run.id,
    });
    // A previous run may have been re-run while this cleanup was queued.
    if (fresh.status !== 'completed') continue;
    for (const artifact of groups.get(run.id)) {
      try {
        await github.rest.actions.deleteArtifact({ ...repo, artifact_id: artifact.id });
      } catch (error) {
        if (error.status !== 404) throw error;
        continue; // It expired or another cleanup already removed it.
      }
      deleted += 1;
      core.info(`Deleted old artifact ${artifact.id} from run ${run.id}`);
    }
  }
  core.info(`Retained all six artifacts from build ${latest.id}; deleted ${deleted} old artifacts.`);
};
