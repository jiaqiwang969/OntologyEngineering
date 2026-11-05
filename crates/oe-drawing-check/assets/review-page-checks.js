document.getElementById('f').onload=()=>{try{
const w=document.getElementById('f').contentWindow,d=w.document,a=w.expressionReview,G=a.graph,checks=[],$=id=>d.getElementById(id);
const ok=(p,label)=>{if(!p)throw Error(label);checks.push(label)};
const count=()=>[...a.visible.values()].filter(x=>x.length).length;
const sourceNodes=()=>d.querySelectorAll('svg>g[data-entity]').length;
const overlays=()=>d.querySelectorAll('#canvas svg line[data-interval]').length;
const same=(x,y)=>JSON.stringify(x)===JSON.stringify(y);
const full=r=>r.length===1&&r[0][0]===0&&r[0][1]===1;
const facts={},NA=[];
const DUP=new Map();for(const m of G.members)for(const x of m.evidence_ids)if(x.startsWith('duplicate_of:'))DUP.set(m.id,x.slice(13));
const isContour=m=>a.exprs.get(m.expression_id).kind==='visible_contour'&&m.member_role==='contour_segment';
const hidden=(x,pred)=>{const p=DUP.get(x.id);return !!p&&a.bySource.get(p).some(y=>pred(y)&&isContour(y))};
const expect=(pred,showDup)=>{const m=new Map();for(const x of G.members){if(!pred(x)||(!showDup&&hidden(x,pred)))continue;if(!m.has(x.source_handle))m.set(x.source_handle,[]);m.get(x.source_handle).push(x.interval)}for(const [k,v] of m)m.set(k,a.union(v));return m};
const partials=m=>[...m.values()].reduce((n,r)=>n+(full(r)?0:r.length),0);
const verify=(m,label)=>{ok(count()===m.size,label+' root coverage');for(const [id,r] of m)ok(same(a.visible.get(id),r),label+' ranges '+id);ok(overlays()===partials(m),label+' partial overlays drawn once each');ok(sourceNodes()===baseNodes,label+' source inventory unchanged')};
const baseNodes=sourceNodes(),N=G.entities.length;
ok(G.schema==='oev2.drawing-expression-graph/1','Typed expression graph loaded');
const isSec=m=>m.evidence_ids.some(x=>x.startsWith('section_profile:')),SEC=[...new Set(G.members.filter(isSec).map(m=>m.source_handle))].sort(),secOnly=SEC.filter(h=>a.bySource.get(h).every(isSec));
const svg0=d.querySelector('#canvas>svg'),ox0=Number(svg0.dataset.drawingOriginX),ty0=Number(svg0.dataset.drawingTopY),LINES=G.structural_lines||[],IMP=G.implied_spans||[];
const segD=(q,a,b)=>{const vx=b[0]-a[0],vy=b[1]-a[1],L=vx*vx+vy*vy,t=L<1e-18?0:Math.max(0,Math.min(1,((q[0]-a[0])*vx+(q[1]-a[1])*vy)/L));return Math.hypot(q[0]-a[0]-t*vx,q[1]-a[1]-t*vy)};
// Drawn structural lines: one element per line, the inferred spans each inside exactly one drawn line, their source end points
// on it within the band of one drawn line (0.02 drawing units).
const drawnLines=()=>{const nodes=[...d.querySelectorAll('#canvas svg [data-line]')],ids=nodes.map(n=>n.dataset.line),once=new Set(ids).size===ids.length,byId=new Map(LINES.map(l=>[l.id,l]));let solid=true,dashed=true,endsOk=true,maxEnd=0;const inferred=[];
 for(const n of nodes){const l=byId.get(n.dataset.line),dash=n.getAttribute('stroke-dasharray');if(l.line_class==='visible_outline'&&dash&&dash!=='none')solid=false;if(l.line_class==='hidden_outline'&&!(dash&&dash!=='none'))dashed=false;
  for(const id of (n.dataset.inferred||'').split(' ').filter(Boolean)){inferred.push(id);const s=IMP.find(x=>x.id===id),pts=[[s.points[0]-ox0,ty0-s.points[1]],[s.points[2]-ox0,ty0-s.points[3]]];
   for(const q of pts){let dd;if(n.tagName==='line')dd=segD(q,[+n.getAttribute('x1'),+n.getAttribute('y1')],[+n.getAttribute('x2'),+n.getAttribute('y2')]);else if(l.arc)dd=Math.abs(Math.hypot(q[0]-(l.arc[0]-ox0),q[1]-(ty0-l.arc[1]))-l.arc[2]);else dd=Infinity;maxEnd=Math.max(maxEnd,dd);if(!(dd<=0.02+1e-9))endsOk=false}}}
 return {nodes:nodes.length,ids,once,solid,dashed,inferred,inferredOnce:new Set(inferred).size===inferred.length,endsOk,maxEnd,overlay:d.querySelectorAll('#canvas svg [data-implied]').length}};
const carriedHidden=()=>{const shown=new Set(drawnLines().ids);return G.members.every(m=>{const l=a.memberLine.get(m.id);if(!l||!shown.has(l.id))return true;return !(a.visible.get(m.source_handle)||[]).some(r=>Math.min(r[1],m.interval[1])-Math.max(r[0],m.interval[0])>1e-9)})};
ok($('complete').checked,'Complete structure is the default');const dl=drawnLines(),dImp=dl.inferred.length;
ok(dl.nodes===LINES.length&&dl.once,'Default view draws every structural line once as one element');ok(dl.solid,'Visible-outline lines are drawn solid');ok(dl.dashed,'Hidden-outline lines are drawn dashed');
ok(dl.overlay===0,'No separate overlay for inferred spans');ok(dImp===IMP.length&&dl.inferredOnce,'Default view draws every inferred span inside one structural line');ok(dl.endsOk,'Inferred spans lie on their drawn lines at their source end points');
ok(carriedHidden(),'Source spans carried by a structural line are not drawn again');ok(sourceNodes()===baseNodes,'Complete structure keeps the source inventory');
ok(secOnly.every(h=>!(a.visible.get(h)||[]).length),'Default view hides section-only sources');
facts.default_view={complete_checked:$('complete').checked,implied_drawn:dImp,section_profile_sources:SEC,section_only_hidden:secOnly.filter(h=>!(a.visible.get(h)||[]).length).length};
facts.structural_lines={lines:LINES.length,drawn:dl.nodes,drawn_once:dl.once,visible_solid:dl.solid,hidden_dashed:dl.dashed,implied_overlay:dl.overlay,inferred_in_lines:dImp,inferred_endpoints_ok:dl.endsOk,inferred_endpoint_max_distance:dl.maxEnd,carried_not_drawn_again:carriedHidden(),
 visible_dasharray:[...d.querySelectorAll('#canvas svg [data-line]')].filter(n=>LINES.find(l=>l.id===n.dataset.line).line_class==='visible_outline').map(n=>n.getAttribute('stroke-dasharray')).filter(Boolean).length};
$('complete').click();ok(!$('complete').checked&&d.querySelectorAll('#canvas svg [data-implied],#canvas svg [data-line]').length===0,'Unticking shows the source as drawn');ok(SEC.every(h=>(a.visible.get(h)||[]).length>0),'As drawn shows every section-profile source');facts.default_view.as_drawn_shows_section_profile=SEC.every(h=>(a.visible.get(h)||[]).length>0);
facts.kind_labels=Object.fromEntries(a.presentKinds.map(k=>[k,d.querySelector('[data-kind="'+k+'"]').closest('label').querySelector('.name').childNodes[0].textContent]));
const all=expect(()=>true,false),dupMembers=G.members.filter(m=>DUP.has(m.id)),hiddenNow=dupMembers.filter(m=>hidden(m,()=>true));
ok(count()===all.size,'Every source drawn with every filter selected; duplicate spans drawn once by their primary');
for(const [id,r] of all)ok(same(a.visible.get(id),r),'Default ranges '+id);
ok(overlays()===partials(all),'Partial overlays only where a duplicate span is left to its primary');
ok(hiddenNow.length===dupMembers.length,'Every duplicate span is drawn by its primary by default');
for(const m of hiddenNow){const r=a.visible.get(m.source_handle)||[],again=r.some(s=>s[0]<=m.interval[0]+1e-9&&s[1]>=m.interval[1]-1e-9),other=G.members.some(y=>y.source_handle===m.source_handle&&y.id!==m.id&&!DUP.has(y.id)&&y.interval[0]<=m.interval[0]+1e-9&&y.interval[1]>=m.interval[1]-1e-9);ok(!again||other,'Duplicate span not drawn again '+m.source_handle)}
facts.duplicates={members:dupMembers.length,hidden_by_default:hiddenNow.length,sources_not_drawn_by_default:N-all.size,primaries:[...new Set(DUP.values())].length};
$('showDuplicates').click();
ok(count()===N,'All source roots visible with duplicates shown');
ok([...a.visible.values()].every(full),'Complete member coverage yields one full domain per root');
ok(overlays()===0,'No partial overlay when every member is visible');
$('showDuplicates').click();ok(count()===all.size,'Hiding duplicates again restores draw-once');
ok(G.expressions.every(e=>e.engineering_target==null),'Part feature targets stay unknown');
ok(new Set(G.members.map(m=>m.source_handle)).size===N,'Every root has at least one member');
$('kindNone').click();ok(count()===0&&!$('empty').hidden,'Kind none hides all and shows empty state');
for(const kind of a.presentKinds){$('kindNone').click();d.querySelector('[data-kind="'+kind+'"]').click();verify(expect(m=>a.exprs.get(m.expression_id).kind===kind),'Kind '+kind)}
$('kindAll').click();$('tab-line').click();ok(a.mode==='line'&&!$('panel-line').hidden&&$('panel-kind').hidden,'Line-type mode changes panel and colour basis');
for(const lt of a.presentLines){$('lineNone').click();d.querySelector('[data-line="'+lt+'"]').click();verify(expect(m=>m.line_type===lt),'Line type '+lt)}
$('lineAll').click();$('tab-prim').click();ok(a.mode==='prim','Primitive mode');
for(const pr of a.presentPrims){$('primNone').click();d.querySelector('[data-prim="'+pr+'"]').click();ok(count()===expect(m=>a.entities.get(m.source_handle).kind===pr).size,'Primitive coverage '+pr)}
$('primAll').click();$('tab-view').click();ok(a.mode==='view','View mode');
for(const v of a.presentViews){$('viewNone').click();d.querySelector('[data-view="'+v+'"]').click();verify(expect(m=>(a.exprs.get(m.expression_id).view_id||'none')===v),'View '+v)}
$('viewAll').click();$('tab-kind').click();
if(a.presentKinds.includes('visible_contour')&&a.presentLines.includes('thick_continuous')){$('kindNone').click();d.querySelector('[data-kind="visible_contour"]').click();$('tab-line').click();$('lineNone').click();d.querySelector('[data-line="thick_continuous"]').click();
verify(expect(m=>a.exprs.get(m.expression_id).kind==='visible_contour'&&m.line_type==='thick_continuous'),'Intersection visible_contour x thick_continuous');
$('lineAll').click();$('tab-kind').click();$('kindAll').click()}else NA.push('visible_contour x thick_continuous intersection');
const shared=G.entities.find(e=>e.geometry_kind==='line'&&!a.bySource.get(e.id).some(m=>DUP.has(m.id))&&a.bySource.get(e.id).some(m=>a.exprs.get(m.expression_id).kind==='visible_contour'&&(m.interval[0]>0||m.interval[1]<1))&&a.bySource.get(e.id).some(m=>a.exprs.get(m.expression_id).kind!=='visible_contour'));
const svg=d.querySelector('#canvas>svg'),ox=Number(svg.dataset.drawingOriginX),ty=Number(svg.dataset.drawingTopY),near=(x,y)=>Math.abs(x-y)<1e-8;
if(shared){ok(!!shared,'A bounded shared contour source exists in the graph');
$('kindNone').click();d.querySelector('[data-kind="visible_contour"]').click();
const rr=a.visible.get(shared.id),nodes=[...d.querySelectorAll('#canvas svg line[data-entity]')].filter(n=>n.dataset.entity===shared.id),pc=shared.coordinates;
ok(rr.length>0&&!full(rr)&&nodes.length===rr.length,'Bounded contour span shown alone, once per range');
for(let i=0;i<nodes.length;i++){const n=nodes[i],[u,v]=rr[i];ok(near(+n.getAttribute('x1'),pc[0]+u*(pc[2]-pc[0])-ox)&&near(+n.getAttribute('y1'),ty-pc[1]-u*(pc[3]-pc[1])),'Span start on the original source domain');ok(near(+n.getAttribute('x2'),pc[0]+v*(pc[2]-pc[0])-ox)&&near(+n.getAttribute('y2'),ty-pc[1]-v*(pc[3]-pc[1])),'Span end on the original source domain')}
$('kindAll').click();ok(full(a.visible.get(shared.id))&&![...d.querySelectorAll('#canvas svg line[data-entity]')].some(n=>n.dataset.entity===shared.id),'All kinds restore the whole source without duplicate overlay')}else NA.push('bounded shared contour source');
const dims=G.expressions.filter(e=>e.family==='dimension'||e.family==='tolerance').sort((x,y)=>a.exprMembers.get(y.id).length-a.exprMembers.get(x.id).length);
const target=dims[0]||[...G.expressions].sort((x,y)=>a.exprMembers.get(y.id).length-a.exprMembers.get(x.id).length)[0];if(!dims.length)NA.push('annotation expression (the largest expression is focused instead)');const btn=d.querySelector('#exprList button[data-expr="'+target.id+'"]');ok(!!btn,'Expression list offers the largest annotation expression');btn.click();
ok(a.selection.type==='expr'&&a.selection.id===target.id,'Expression focus selected from list');
const memberSrc=new Set(a.exprMembers.get(target.id).map(m=>m.source_handle));
for(const id of memberSrc){const g=[...d.querySelectorAll('svg>g[data-entity]')].filter(n=>n.dataset.entity===id);ok(g.length===0||g.every(n=>n.classList.contains('selected')),'Focused member marked '+id)}
const other=[...d.querySelectorAll('svg>g[data-entity]')].find(n=>!memberSrc.has(n.dataset.entity)&&n.style.display!=='none');ok(!!other,'A visible non-member exists while focusing');const op0=other.style.getPropertyValue('--op');ok(op0==='1','Non-members keep full opacity while focusing (no default dimming)');
$('dimOthers').click();const op1=other.style.getPropertyValue('--op');ok(op1==='.2','Dimming option dims non-members');$('dimOthers').click();const op2=other.style.getPropertyValue('--op');ok(op2==='1','Clearing the option restores full opacity');facts.dimming={focused:target.id,non_member:other.dataset.entity,default_opacity:op0,option_opacity:op1,after_clear:op2};
ok($('details').textContent.includes(target.rule)&&$('details').textContent.includes(target.label.slice(0,6)),'Expression details show rule and label');
$('clear').click();ok(a.selection===null,'Selection cleared');
const known=G.entities.find(e=>e.kind==='DIMENSION')||G.entities[0];
$('search').value=known.id;$('locate').click();ok(a.selection&&a.selection.type==='source'&&a.selection.id===known.id,'Handle lookup selects the source');
ok($('details').textContent.includes(known.id),'Source details keep handle identity');const CMPX=JSON.parse(d.getElementById('payload').textContent).comparison||{};
ok(!Object.keys(CMPX).length&&$('compare').textContent.includes('没有嵌入独立参照'),'No independent reference embedded and the comparison card says so');
$('search').value='';$('search').dispatchEvent(new w.Event('input'));ok(d.querySelectorAll('#exprList button[data-expr]').length===G.expressions.length,'Cleared search lists every expression');
$('kindNone').click();$('original').click();ok(count()===N,'Raw comparison shows complete source');$('original').click();ok(count()===0,'Leaving raw comparison restores filters');
$('kindAll').click();ok(d.querySelectorAll('#canvas svg [data-implied],#canvas svg [data-line]').length===0,'As drawn: no implied span and no structural line');
$('complete').click();const cl=drawnLines();ok(cl.nodes===LINES.length&&cl.once&&cl.overlay===0&&cl.inferred.length===IMP.length,'Complete structure draws every structural line once, inferred spans inside them');
const impOk=cl.endsOk;ok(impOk,'Implied spans drawn at their source end points');ok(secOnly.every(h=>!(a.visible.get(h)||[]).length),'Complete structure hides section-only sources');const nComplete=cl.inferred.length;ok(sourceNodes()===baseNodes,'Complete structure keeps the source inventory');
const hl=LINES.find(l=>l.line_class==='hidden_outline');if(hl){$('kindNone').click();d.querySelector('[data-kind="hidden_contour"]').click();const hn=[...d.querySelectorAll('#canvas svg [data-line]')];ok(hn.length===LINES.filter(l=>l.line_class==='hidden_outline').length&&hn.every(n=>LINES.find(l=>l.id===n.dataset.line).line_class==='hidden_outline'),'Kind filter selects the structural lines of its class');$('kindAll').click()}
const tl=LINES.find(l=>l.trimmed.length);if(tl){a.chooseLine(tl.id);ok($('details').textContent.includes(tl.id)&&$('details').textContent.includes('截下'),'Line details show the trimmed part');$('clear').click()}
$('complete').click();ok(d.querySelectorAll('#canvas svg [data-implied],#canvas svg [data-line]').length===0,'Leaving complete structure removes structural lines');
$('openEnds').click();const mk=[...d.querySelector('#canvas svg').lastElementChild.children];ok(mk.length===(G.structure_ends||[]).length,'End markers for every reported end');
const byColor={};for(const m of mk)byColor[m.getAttribute('stroke')]=(byColor[m.getAttribute('stroke')]||0)+1;$('openEnds').click();
facts.topology={as_drawn_implied:0,complete_implied:nComplete,implied_endpoints_ok:impOk,end_markers:mk.length,marker_colors:byColor};
const multi=id=>new Set(a.bySource.get(id).map(m=>m.expression_id)).size>1,hiddenS=x=>{const p=DUP.get(x.id);return !!p&&multi(p)&&a.bySource.get(p).some(isContour)};
$('shared').click();ok(count()===[...a.bySource.keys()].filter(id=>multi(id)&&a.bySource.get(id).some(m=>!hiddenS(m))).length,'Shared filter uses only the graph');$('shared').click();
const old=a.view;$('zoomIn').click();ok(a.view[2]<old[2],'Zoom changes viewport');$('fit').click();ok(a.view[2]>0,'Fit restores finite viewport');
$('note').value='Browser verification note; not an engineering conclusion.';$('saveNote').click();ok(a.noteCount>=1&&a.exportContent().schema==='oev2.drawing-expression-feedback/1','Feedback saved separately from the graph');
ok(sourceNodes()===baseNodes,'All toggles keep the source node inventory');
const mode='__MODE__';if(!$('complete').checked)$('complete').click();if(mode==='line'){$('tab-line').click()}else if(mode==='view'){$('tab-view').click()}else if(mode==='geometry'){$('kindGeometry').click()}else if(mode==='complete'){$('kindGeometry').click();$('openEnds').click()}else if(mode==='asdrawn'){$('kindGeometry').click();$('complete').click()}else if(mode==='focus'){const fl=__FOCUS__,f=(fl?G.expressions.filter(e=>e.label.includes(fl)).sort((x,y)=>a.exprMembers.get(y.id).length-a.exprMembers.get(x.id).length)[0]:null)||G.expressions.find(e=>e.rule==='dimension_line_of_feature_of_size_with_attached_frame')||target;d.querySelector('#exprList button[data-expr="'+f.id+'"]').click();facts.screenshot_focus={id:f.id,label:f.label,kind:f.kind}}
if(mode!=='focus')$('fit').click();const box=__BOX__;if(box){a.setView([box[0]-ox,ty-box[3],box[2]-box[0],box[3]-box[1]]);facts.screenshot_region=box}
facts.not_applicable=NA;document.getElementById('checks').textContent=JSON.stringify({status:'passed',count:checks.length,checks,facts,sourceNodes:baseNodes,roots:N,expressions:G.expressions.length,visibleAtScreenshot:count(),mode});
}catch(e){document.getElementById('checks').textContent=JSON.stringify({status:'failed',error:String(e),stack:e.stack})}};