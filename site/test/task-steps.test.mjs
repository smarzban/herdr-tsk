import test from 'node:test';
import assert from 'node:assert/strict';
import { TaskSteps } from '../public/task-steps.js';
const task = () => ({id:'task', status:'started', steps:[{id:'one',text:'original',done:false}]});
test('canceling a staged rename preserves independent additions and completion',()=>{
 const t=task(), s=new TaskSteps();
 s.begin(t,'one'); s.editor.text='renamed'; s.save(t,false);
 assert.equal(t.steps[0].text,'original');
 s.toggle(t); s.begin(t); s.editor.text='new'; s.save(t,false); s.cancel(); s.cancel();
 assert.deepEqual(t.steps.map(x=>[x.text,x.done]),[['original',true],['new',false]]);
 assert.equal(t.status,'started');
});
test('delete requires consecutive presses and staged removal can be canceled',()=>{
 const t=task(), s=new TaskSteps(); s.move(t,1);
 assert.equal(s.remove(t),false); s.toggle(t);
 assert.equal(s.remove(t),false); assert.equal(t.steps.length,1);
 s.begin(t,'one'); s.editor.text='changed'; s.save(t,false);
 s.remove(t); s.remove(t); assert.equal(s.rows(t).length,0);
 s.cancel(); assert.equal(t.steps.length,1);
});
test('reverse selection starts at add; blank additions refuse without mutation',()=>{
 const t=task(),s=new TaskSteps(); s.move(t,-1); assert.equal(s.selected,'add');
 s.begin(t); s.editor.text='  '; assert.equal(s.save(t,true),false); assert.equal(t.steps.length,1);
});
