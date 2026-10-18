import test from 'node:test';
import assert from 'node:assert/strict';
import { withGuard } from './guard-fixture.mjs';
import { MAX_FRAME, WireClient } from './guard-wire.ts';
const inputCall={serviceId:'pi.agent-controller',member:'prompt',args:[]};
async function until(predicate) {
 const deadline=Date.now()+3000;
 while(!predicate()){if(Date.now()>deadline)throw Error('fixture deadline');await new Promise(r=>setTimeout(r,5));}
}
test('operator wire requires a distinct decision and reclaims disconnected input by native ID',async()=>{
 await withGuard(async({guard,operator,session})=>{
  const input=guard.input(session), cancelled=[];
  input.register('fixture-native-a',async(id)=>{cancelled.push(id);},async()=>{});
  const requester=await WireClient.connect(guard.operatorPath);
  const stranger=await WireClient.connect(guard.operatorPath);
  try {
   await requester.call('subscribe');
   const request=requester.call('input_request',{session,attachment:'fixture-native-a',label:'fixture controller'});
   await until(()=>input.view().pending);
   const id=input.view().pending.id;
   await assert.rejects(requester.call('input_decide',{session,id,allow:true}),/self-approved/);
   await operator.call('input_decide',{session,id,allow:true});
   assert.equal((await request).granted,true);
   await assert.rejects(stranger.call('input_release',{session}),/another connection/);
   const admission=await input.before('fixture-native-a',inputCall);
   input.assertCurrent(admission);
   await input.after(admission,inputCall,{accepted:true,operationId:'97'});
   requester.close();
   await until(()=>input.view().holder===null && cancelled.length===1);
   assert.deepEqual(cancelled,['97']);
   await assert.rejects(input.before('fixture-native-a',inputCall),/lease required/);
  } finally {requester.close();stranger.close();}
 });
});
test('oversized observation retires its lease owner without an uncaught host exception',async()=>{
 await withGuard(async({guard,operator,session})=>{
  const input=guard.input(session);
  input.register('fixture-native-b',async()=>{},async()=>{});
  const requester=await WireClient.connect(guard.operatorPath);
  const original=guard.state.bind(guard);
  try {
   await requester.call('subscribe');
   const request=requester.call('input_request',{session,attachment:'fixture-native-b',label:'fixture controller'});
   await until(()=>input.view().pending);
   await operator.call('input_decide',{session,id:input.view().pending.id,allow:true});
   assert.equal((await request).granted,true);
   guard.state=()=>({...original(),oversized:'x'.repeat(MAX_FRAME)});
   assert.doesNotThrow(()=>guard.emit());
   assert.equal(input.view().holder,null);
   guard.state=original;
   await assert.rejects(input.before('fixture-native-b',inputCall),/lease required/);
  } finally {guard.state=original;requester.close();}
 });
});
