import assert from 'node:assert/strict';
import { mkdtempSync, rmSync, symlinkSync, readFileSync, writeFileSync, cpSync, readdirSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { spawnSync } from 'node:child_process';
import { pathToFileURL } from 'node:url';
import test from 'node:test';

const source = process.env.OQT_PI_SOURCE ?? '/home/wismut/.local/share/oqt/pi-source';
const root = resolve(import.meta.dirname, '../..');
const pin = 'cdaf6c227d4718a72b59ccd02152c9614df44cfc';
function run(command, args, options = {}) {
 const result = spawnSync(command, args, { encoding: 'utf8', ...options });
 assert.equal(result.status, 0, `${command}: ${result.stdout}\n${result.stderr}`);
 return result.stdout;
}

if (process.argv[2] === '--native') {
 const snapshot = process.argv[3];
 const load = (path) => import(pathToFileURL(join(snapshot, path)).href);
 const { renderPermission, permissionDisplayText, permissionHelpCommand } = await load('packages/coding-agent/examples/plugins/oq-permissions/src/permission-render.ts');
 const { permissionFeedback } = await load('packages/coding-agent/src/experimental/permission-feedback.ts');
 const { createToolPresentation, escapeToolPresentation } = await load('packages/coding-agent/src/experimental/services/tool-presentation.ts');
 const { ToolExecutionComponent } = await load('packages/coding-agent/src/modes/interactive/components/tool-execution.ts');
 const { initTheme } = await load('packages/coding-agent/src/modes/interactive/theme/theme.ts');
 const { visibleWidth } = await import('@earendil-works/pi-tui');
 initTheme('dark', false);
 const registry = createToolPresentation();
 const old = registry.register('request_access', () => ({title:'old',lines:[]}));
 const dispose = registry.register('request_access', renderPermission);
 old(); // retiring generation cannot remove replacement
 const args = { path: '/tmp/evil\u001b]52;c;PAYLOAD\u0007\r\u009b2J\u202e\nfile', access:'read', reason:'read\u001b[2J\b\u200bwhy' };
 assert.equal(permissionDisplayText(args.path), escapeToolPresentation(args.path));
 assert.doesNotMatch(permissionDisplayText(args.path), /[\x00-\x1f\x7f-\x9f\u202e]/);
 let selected; let reviews = 0; const statuses = []; let explanations = 0;
 const help = permissionHelpCommand({list:()=>[{name:'permissions',run:(args)=>{assert.equal(args,'');reviews++;}}]}, {
  select:async (title,items,defaultValue)=>{if(defaultValue==='close'){explanations++;assert.match(title,/entire tree/); assert.match(title,/do not confirm new authorization/); assert.doesNotMatch(title,/grant nothing/); assert.deepEqual(items,[{value:'close',label:'Close explanation'}]);return 'close';}assert.equal(defaultValue,'explain_scope'); assert.deepEqual(items.map(x=>x.value),['explain_scope','review_permissions']);return selected;},
  showStatus:(message)=>statuses.push(message),
 });
 for (selected of [undefined,'untrusted shell string','explain_scope']) await help.run('',{});
 assert.equal(reviews,0); assert.equal(statuses.length,0); assert.equal(explanations,1);
 selected='review_permissions'; await help.run('',{}); assert.equal(reviews,1);
 const ui = {requestRender() {}};
 const card = new ToolExecutionComponent('request_access', 'exact-id', args,
  {presentation: (input) => registry.render('request_access', input)}, undefined, ui, '/tmp');
 const clean = (lines) => lines.join('\n').replace(/\x1b\[[0-9;]*m/g, '');
 const snapshots = {};
 function check(label, expected) {
  const rendered = card.render(80);
  for (const line of rendered) assert.ok(visibleWidth(line) <= 80, `${label}: exceeds 80 columns`);
  const text = clean(rendered);
  snapshots[label] = text.split('\n').map(line=>line.trimEnd()).join('\n');
  assert.match(text, expected);
  assert.doesNotMatch(text, /[\x00-\x09\x0b-\x1f\x7f-\x9f\u202e\u200b]/);
  return text;
 }
 check('preparing', /preparing/);
 card.setArgsComplete(); card.markExecutionStarted();
 assert.doesNotMatch(check('pending', /Checking policy and approval/),/awaiting operator|prompted/);
 for (const [code, expected] of Object.entries({protected_safety:/safety rule/, configured_ceiling:/configured policy/,operator_denied:/Declined by the operator/,unavailable:/Approval unavailable/,expired:/expired/,stale:/Request changed/,cancelled:/cancelled/,unknown:/Authorization not confirmed/})) {
  card.updateResult({content:[{type:'text',text:'User denied\u001b]52;c;EVIL\u0007'}], isError:true, details:{permissionFeedback:permissionFeedback({feedback:{code}})}});
  const text = check(code, expected);
  if (code !== 'operator_denied') assert.doesNotMatch(text, /Declined by the operator/);
  if (['unknown','expired','unavailable'].includes(code)) { assert.match(text,/does not confirm new authorization/); assert.match(text,/current grants/); assert.doesNotMatch(text,/grant nothing/); }
 }
 card.updateResult({content:[{type:'text',text:'operator declined'}],isError:true});
 check('unstructured error', /Authorization not confirmed/);
 card.updateResult({content:[],isError:true,details:{permissionFeedback:permissionFeedback({feedback:{code:'operator_denied'}},'uncertain')}});
 const uncertain = check('uncertain', /Effects uncertain/);
 assert.doesNotMatch(uncertain,/operation was not started/);
 assert.doesNotMatch(uncertain,/Declined by the operator/);
 card.updateResult({content:[{type:'text',text:'ok'}],isError:false, details:{granted:{capability:'read',resource:'/tmp/tree'},scope:'tree',expiresAt:0}});
 let text = check('success', /Recorded grant outcome/);
 assert.match(text, /entire tree/); assert.doesNotMatch(text, /"arguments"/);
 assert.match(text,/Recorded expiry: 1970-01-01T00:00:00.000Z/);
 assert.match(text.replace(/\s+/g,' '),/Historical or imported results do not restore a grant/);
 const historical = renderPermission({args:{path:'/tmp/tree'},phase:'complete',result:{isError:false,details:{granted:{capability:'read',resource:'/tmp/tree'},expiresAt:Number.NaN}}});
 assert.equal(historical.title,'Recorded grant outcome');
 assert.ok(!historical.lines.some(line=>line.includes('expiry')));
 assert.ok(historical.lines.some(line=>line.includes('/permissions shows current access')));
 card.handleMouse({type:'click',button:'left',x:2,y:2,width:80,height:card.render(80).length});
 text = check('native mouse-expanded', /Raw details/); assert.match(text, /"arguments"/);
 assert.match(text, /\\u001b/);
 card.handleMouse({type:'click',button:'left',x:2,y:2,width:80,height:card.render(80).length});
 assert.doesNotMatch(check('native mouse-collapsed', /Recorded grant outcome/), /"arguments"/);
 // Host sanitizes arbitrary plugin summaries too, not only our renderer.
 registry.register('request_access',()=>({title:'Bad\u001b[2J',lines:['reason\r\u009b2J\u202e']}));
 card.invalidate(); check('host defensive sanitizer', /Bad/);
 // Throwing registry contributions fail safe, without generic raw JSON fallback.
 registry.register('request_access', () => {throw new Error('unsafe\u001b[2J');});
 card.invalidate();
 check('broken renderer', /Presentation unavailable/);
 dispose();
 const fixture = join(import.meta.dirname,'permission-render.snapshots.json');
 if (process.env.OQT_UPDATE_RENDER_SNAPSHOTS === '1') writeFileSync(fixture,JSON.stringify(snapshots,null,2)+'\n');
 else assert.deepEqual(snapshots,JSON.parse(readFileSync(fixture,'utf8')));
 console.log('native rendering cases passed');
} else {
 test('exact pinned derivative, guarded anchors, compile and native rendering', () => {
  assert.equal(run('git', ['-C',source,'rev-parse','HEAD']).trim(),pin);
  const temp = mkdtempSync(join(tmpdir(),'oqt-permission-render-'));
  try {
   run('bash',['-c','git -C "$1" archive "$2" | tar -x -C "$3"','archive',source,pin,temp]);
   symlinkSync(join(source,'node_modules'),join(temp,'node_modules'),'dir');
   cpSync(join(source,'packages/ai/src/providers/data'),join(temp,'packages/ai/src/providers/data'),{recursive:true});
   const baseline = spawnSync(join(source,'node_modules/.bin/tsc'),['--noEmit','--pretty','false'],{cwd:temp,encoding:'utf8'});
   const apply = `import sys\nfrom pathlib import Path\nsys.path.insert(0, ${JSON.stringify(join(root,'tools/oqt'))})\nimport tool_presentation_patches as p\np.apply(Path(${JSON.stringify(temp)}))\n`;
   // Parent applies existing source patches first: prove anchors coexist.
   run('python3',['-c',`import sys\nfrom pathlib import Path\nsys.path.insert(0, ${JSON.stringify(join(root,'tools/oqt'))})\nimport source_patches\nsource_patches.apply(Path(${JSON.stringify(temp)}))\n`]);
   const before = readFileSync(join(temp,'packages/coding-agent/src/experimental/client-tui.ts'),'utf8');
   const again = spawnSync('python3',['-c',apply],{encoding:'utf8'});
   assert.notEqual(again.status,0); assert.match(again.stderr,/refusing existing/);
   assert.equal(readFileSync(join(temp,'packages/coding-agent/src/experimental/client-tui.ts'),'utf8'),before);
   cpSync(join(source,'packages/ai/src/providers/data'),join(temp,'packages/ai/src/providers/data'),{recursive:true});
   for (const file of readdirSync(join(root,'tools/oqt')).filter((name) => name.endsWith('.ts'))) cpSync(join(root,'tools/oqt',file),join(temp,'packages/coding-agent/src/experimental',file));
   cpSync(join(root,'tools/oqt/plugin'),join(temp,'packages/coding-agent/examples/plugins/oq-permissions'),{recursive:true});
   const esbuild = join(source,'node_modules/.bin/esbuild');
   for (const entry of ['packages/coding-agent/src/experimental/client-tui.ts', 'packages/coding-agent/src/experimental/plugin.ts', 'packages/coding-agent/examples/plugins/oq-permissions/src/tui.ts']) {
    run(esbuild,[join(temp,entry),'--bundle','--platform=node','--format=esm','--packages=external','--alias:@earendil-works/pi-coding-agent/experimental/plugin='+join(temp,'packages/coding-agent/src/experimental/plugin.ts'),'--outfile='+join(temp,'compiled-'+entry.split('/').at(-1)+'.mjs')]);
   }
   // Full native derivative typecheck: compare exact diagnostics with the pinned
   // baseline rather than silently dropping unrelated SDK dependency failures.
   const derivative = spawnSync(join(source,'node_modules/.bin/tsc'),['--noEmit','--pretty','false'],{cwd:temp,encoding:'utf8'});
   assert.equal(derivative.status,baseline.status,derivative.stdout+derivative.stderr);
   assert.equal(derivative.stdout,baseline.stdout,'Derivative introduced TypeScript diagnostics:\n'+derivative.stdout);
   assert.equal(derivative.stderr,baseline.stderr);
   writeFileSync(join(tmpdir(),'oqt-permission-render-tsc.log'),`BASELINE status=${baseline.status}\n${baseline.stdout}${baseline.stderr}\nDERIVATIVE status=${derivative.status}\n${derivative.stdout}${derivative.stderr}`);
   for (const noColor of [undefined,'1']) {
    const env = {...process.env,NODE_PATH:join(source,'node_modules')};
    if (noColor) env.NO_COLOR=noColor; else delete env.NO_COLOR;
    run('bun',[import.meta.filename,'--native',temp],{env,cwd:source});
   }
   // Altered anchor fails before ANY file is written.
   rmSync(join(temp,'packages/coding-agent/src/experimental/services/tool-presentation.ts'));
   const broken = join(temp,'packages/coding-agent/src/experimental/plugin.ts');
   writeFileSync(broken,'changed upstream');
   const failed = spawnSync('python3',['-c',apply],{encoding:'utf8'});
   assert.notEqual(failed.status,0); assert.match(failed.stderr,/Upstream changed/);
   assert.equal(readFileSync(join(temp,'packages/coding-agent/src/experimental/client-tui.ts'),'utf8'),before);
  } finally { if (process.env.OQT_KEEP_RENDER_SNAPSHOT === '1') console.log('Derivative snapshot:',temp); else rmSync(temp,{recursive:true,force:true}); }
 });
}
