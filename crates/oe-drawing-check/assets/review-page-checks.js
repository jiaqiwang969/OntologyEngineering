const run=()=>{try{
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
const verify=(m,label)=>{ok(count()===m.size,label+' root coverage');for(const [id,r] of m)ok(same(a.visible.get(id),r),label+' ranges '+id);ok(overlays()===partials(m),label+' partial overlays drawn once each');ok(sourceNodes()===baseNodes,label+' source inventory unchanged');const s=shownGroups(label);checks.push(label+' groups shown by their claiming members, empty notice only when nothing is drawn');return s};
const baseNodes=sourceNodes(),N=G.entities.length;
ok(G.schema==='oev2.drawing-expression-graph/1','Typed expression graph loaded');
const isSec=m=>m.evidence_ids.some(x=>x.startsWith('section_profile:')),SEC=[...new Set(G.members.filter(isSec).map(m=>m.source_handle))].sort(),secOnly=SEC.filter(h=>a.bySource.get(h).every(isSec));
const svg0=d.querySelector('#canvas>svg'),ox0=Number(svg0.dataset.drawingOriginX),ty0=Number(svg0.dataset.drawingTopY),LINES=G.structural_lines||[],IMP=G.implied_spans||[];
// The drawing as embedded: the same page parsed with scripting disabled. The converted order, the converter's stroke widths and
// the member identities are read from it, never from the page under test.
const pd=document.getElementById('p').contentDocument,PRIS=[...pd.querySelectorAll('#canvas>svg>g[data-entity]')],live=()=>[...d.querySelectorAll('#canvas>svg>g[data-entity]')];
const shape=g=>{const n=g.firstElementChild;return g.children.length+'|'+(n?n.tagName+'|'+(n.getAttribute('points')||n.getAttribute('d')||[n.getAttribute('cx'),n.getAttribute('cy'),n.getAttribute('r')].join(',')):'')};
ok(!document.getElementById('p').contentWindow.expressionReview&&PRIS.length===baseNodes,'The embedded drawing is read from a copy of the page with scripting disabled');
const SEQ=new Map();for(const g of live())SEQ.set(g,Number(g.dataset.seq));
ok(new Set(SEQ.values()).size===baseNodes&&[...SEQ].every(([g,i])=>{const p=PRIS[i];return !!p&&g.dataset.entity===p.dataset.entity&&(g.dataset.member||'')===(p.dataset.member||'')&&shape(g)===shape(p)}),'Every drawn group is a group of the embedded drawing and carries its converted position');
// The graph, indexed here; the page's own indexes are not used for the expectations below.
const ENT=new Map(G.entities.map(e=>[e.id,e])),MS=new Map(G.entities.map(e=>[e.id,[]]));for(const m of G.members)MS.get(m.source_handle).push(m);
const EXK=new Map(G.expressions.map(e=>[e.id,e.kind])),EXV=new Map(G.expressions.map(e=>[e.id,e.view_id||'none'])),MLINE=new Map(),LBY=new Map(LINES.map(l=>[l.id,l]));for(const l of LINES)for(const p of [...l.pieces,...l.coincident])if(p.startsWith('member:'))MLINE.set(p.slice(7),l.id);
// Member identity: a group names a native member of its root; a member with native member id X claims the groups whose data-member
// is X or lies under X/, a member without one claims every group of its root, a group without data-member is claimed by every member.
const IDENT=new Set(PRIS.filter(p=>p.dataset.member).map(p=>p.dataset.entity)),NATIVES=new Map();
ok(PRIS.every(p=>{const D=p.dataset.member,r=p.dataset.entity;if(!D)return true;if(!NATIVES.has(r))NATIVES.set(r,new Set(ENT.get(r).member_ids||[]));return D.startsWith(r+'/')&&NATIVES.get(r).has(D)}),'Every data-member names a native member of its root');
const CLAIM=new Map(),claimers=g=>{if(!CLAIM.has(g)){const ms=MS.get(g.dataset.entity),D=g.dataset.member;CLAIM.set(g,D?ms.filter(m=>!m.native_member_id||D===m.native_member_id||D.startsWith(m.native_member_id+'/')):ms)}return CLAIM.get(g)};
ok(PRIS.every(p=>claimers(p).length>0),'Every group of the drawing is claimed by a member of its root');
// The page state, read from the controls and the drawn structural lines; what each root shows follows from the graph.
const U=rr=>{rr=rr.map(r=>[r[0]<1e-9?0:r[0],r[1]>1-1e-9?1:r[1]]).sort((x,y)=>x[0]-y[0]);const o=[];for(const r of rr){const l=o[o.length-1];if(l&&r[0]<=l[1]+1e-9)l[1]=Math.max(l[1],r[1]);else o.push([...r])}return o};
const cut=(rr,cc)=>{let o=rr.map(r=>[...r]);for(const [x,y] of cc){const n=[];for(const [p,q] of o){if(y<=p+1e-9||x>=q-1e-9){n.push([p,q]);continue}if(x>p+1e-9)n.push([p,x]);if(y<q-1e-9)n.push([y,q])}o=n}return o.filter(r=>r[1]-r[0]>1e-9)};
const ticked=axis=>new Set([...d.querySelectorAll('input[data-'+axis+']')].filter(n=>n.checked).map(n=>n.dataset[axis]));
const layer=k=>svg0.children[svg0.children.length-k]; // 4 partial spans, 3 structural lines, 2 highlight, 1 end markers
const pageState=()=>{const raw=$('original').checked,complete=$('complete').checked&&!raw,sharedOnly=$('shared').checked,showDup=$('showDuplicates').checked,en={kind:ticked('kind'),line:ticked('line'),prim:ticked('prim'),view:ticked('view')},lines=new Set([...layer(3).children].map(n=>n.dataset.line));
 const pass=m=>!(isSec(m)&&complete)&&en.kind.has(EXK.get(m.expression_id))&&en.line.has(m.line_type)&&en.prim.has(ENT.get(m.source_handle).kind)&&en.view.has(EXV.get(m.expression_id));
 const multi=id=>new Set(MS.get(id).map(m=>m.expression_id)).size>1,carried=m=>complete&&lines.has(MLINE.get(m.id)),dupHidden=m=>{const p=DUP.get(m.id);return !!p&&!showDup&&(!sharedOnly||multi(p))&&MS.get(p).some(x=>pass(x)&&isContour(x))};
 const cache=new Map(),root=id=>{if(!cache.has(id)){const ms=MS.get(id);let chosen,ranges,byLine=false;if(raw){chosen=ms;ranges=[[0,1]]}else if(sharedOnly&&!multi(id)){chosen=[];ranges=[]}else{chosen=ms.filter(m=>pass(m)&&!dupHidden(m)&&!carried(m));ranges=complete?cut(U(chosen.map(m=>m.interval)),U(ms.filter(carried).map(m=>m.interval))):U(chosen.map(m=>m.interval));byLine=!ranges.length&&ms.some(m=>carried(m)&&pass(m))}
   cache.set(id,{chosen,on:new Set(chosen),ranges,byLine,full:ranges.length===1&&ranges[0][0]===0&&ranges[0][1]===1})}return cache.get(id)};
 return {raw,complete,root}};
// A group is shown exactly when its root is shown whole and a chosen member claims it; the empty notice is shown exactly when
// nothing is drawn (no group, no partial span, no structural line); the status line counts the sources drawn, by themselves or by a line.
const shownGroups=label=>{const S=pageState();let shown=0,named=0,namedShown=0;for(const g of live()){const R=S.root(g.dataset.entity),want=S.raw||(R.full&&claimers(g).some(m=>R.on.has(m))),is=g.style.display!=='none';
  if(want!==is)throw Error(label+': group '+(g.dataset.member||g.dataset.entity)+' is '+(is?'shown':'hidden')+' against the members that claim it');if(is)shown++;if(g.dataset.member){named++;if(is)namedShown++}}
 const spans=layer(4).children.length,lines=layer(3).children.length,drawn=shown+spans+lines;if($('empty').hidden!==(drawn>0))throw Error(label+': the empty notice is '+($('empty').hidden?'hidden':'shown')+' with '+drawn+' pieces drawn');
 let own=0,byLine=0;for(const e of G.entities){const R=S.root(e.id);if(R.ranges.length)own++;else if(R.byLine)byLine++}
 if(!$('status').textContent.startsWith('显示 '+(own+byLine)+' / '+N+' 个来源'))throw Error(label+': the status line does not count '+(own+byLine)+' sources drawn: '+$('status').textContent);
 return {groups_shown:shown,groups_with_member:named,groups_with_member_shown:namedShown,partial_spans:spans,structural_lines:lines,sources_drawn:own,sources_drawn_by_lines_only:byLine,empty_notice:!$('empty').hidden}};
// Widths: two widths 2:1; a stroke is thick only when the member that decides it has a thick line type. The deciding member has the
// best rank; for a group with member identity, at equal rank, the member that names it most specifically. A root that draws several
// native members without member identity is thick only when all its line-typed members are thick. Tolerance 1e-4 relative.
const close=(x,y)=>Math.abs(x-y)<=1e-4*Math.max(1e-3,Math.abs(y)),px=n=>parseFloat(w.getComputedStyle(n).strokeWidth);
const RK=['visible_contour','center_axis','hidden_contour','thread_convention','break_line','phantom_line','section_hatch'],rk=m=>{const k=EXK.get(m.expression_id),i=RK.indexOf(k);return k==='unresolved'?99:i<0?10:i};
const top=(ms,specific)=>{let b=null,br=0,bd=0;for(const m of ms){const r=rk(m),dd=specific&&m.native_member_id?m.native_member_id.split('/').length:0;if(!b||r<br||(r===br&&dd>bd)){b=m;br=r;bd=dd}}return b};
const isThick=lt=>typeof lt==='string'&&lt.startsWith('thick'),lined=lt=>/^(thick|thin)_|^(continuous|wavy|double_break)$/.test(lt),CURVE=new Set(['LINE','ARC','CIRCLE','ELLIPSE','SPLINE','LWPOLYLINE','POLYLINE']);
const wholeThick=id=>{const ms=MS.get(id).filter(m=>lined(m.line_type));return ms.length>0&&ms.every(m=>isThick(m.line_type))};
const DASH=lt=>lt.includes('dash_double_dot')?[24,3,.5,3,.5,3]:lt.includes('dash_dot')?[24,3,.5,3]:lt.includes('dashed')?[12,3]:[];
const strokeWidths=label=>{const S=pageState(),W=a.widths,out={thick:0,thin:0,partial_spans:0,structural_thick:0,structural_thin:0};if(S.raw)throw Error(label+': widths by member are checked in the classified views');
 for(const g of live()){if(g.style.display==='none')continue;const id=g.dataset.entity,R=S.root(id),thick=IDENT.has(id)?isThick(top(claimers(g).filter(m=>R.on.has(m)),!!g.dataset.member)?.line_type):CURVE.has(ENT.get(id).kind)?isThick(top(R.chosen)?.line_type):wholeThick(id);
  for(const n of g.querySelectorAll('[stroke-width]')){const v=px(n);if(!close(v,thick?W.thick:W.thin))throw Error(label+': a stroke of '+(g.dataset.member||id)+' is '+v+' wide, its member rule gives '+(thick?W.thick:W.thin));thick?out.thick++:out.thin++}}
 for(const n of layer(4).children){const [x,y]=n.dataset.interval.split(',').map(Number),R=S.root(n.dataset.entity),thick=isThick(top(R.chosen.filter(m=>Math.min(m.interval[1],y)-Math.max(m.interval[0],x)>1e-9))?.line_type);if(!close(px(n),thick?W.thick:W.thin))throw Error(label+': a partial span of '+n.dataset.entity+' is '+px(n)+' wide');out.partial_spans++}
 for(const n of layer(3).children){const l=LBY.get(n.dataset.line),thick=isThick(l.line_type),own=thick?W.d:W.d/2,want=DASH(l.line_type),have=(n.getAttribute('stroke-dasharray')||'').split(/[ ,]+/).filter(Boolean).map(Number);if(!close(px(n),thick?W.thick:W.thin))throw Error(label+': structural line '+l.id+' is '+px(n)+' wide');
  if(have.length!==want.length||!want.every((u,i)=>close(have[i],u*own)))throw Error(label+': the dashes of '+l.id+' are not multiples of its own width');thick?out.structural_thick++:out.structural_thin++}
 return out};
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
ok(count()===N,'All source roots visible with duplicates shown');ok(live().every(g=>g.style.display!=='none'),'Every group of the drawing visible with every filter selected and duplicates shown');
ok([...a.visible.values()].every(full),'Complete member coverage yields one full domain per root');
ok(overlays()===0,'No partial overlay when every member is visible');
$('showDuplicates').click();ok(count()===all.size,'Hiding duplicates again restores draw-once');
ok(G.expressions.every(e=>e.engineering_target==null),'Part feature targets stay unknown');
ok(new Set(G.members.map(m=>m.source_handle)).size===N,'Every root has at least one member');
$('kindNone').click();ok(count()===0&&!$('empty').hidden,'Kind none hides all and shows empty state');
facts.by_kind={};for(const kind of a.presentKinds){$('kindNone').click();d.querySelector('[data-kind="'+kind+'"]').click();const s=verify(expect(m=>a.exprs.get(m.expression_id).kind===kind),'Kind '+kind);facts.by_kind[kind]={groups_shown:s.groups_shown,groups_with_member_shown:s.groups_with_member_shown}}
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
for(const id of memberSrc){const g=live().filter(n=>n.dataset.entity===id);ok(g.every(n=>n.classList.contains('selected')===claimers(n).some(m=>m.expression_id===target.id)),'Focused member marked on the groups its expression claims '+id)}
const other=[...d.querySelectorAll('svg>g[data-entity]')].find(n=>!memberSrc.has(n.dataset.entity)&&n.style.display!=='none');ok(!!other,'A visible non-member exists while focusing');const op0=other.style.getPropertyValue('--op');ok(op0==='1','Non-members keep full opacity while focusing (no default dimming)');
$('dimOthers').click();const op1=other.style.getPropertyValue('--op');ok(op1==='.2','Dimming option dims non-members');$('dimOthers').click();const op2=other.style.getPropertyValue('--op');ok(op2==='1','Clearing the option restores full opacity');facts.dimming={focused:target.id,non_member:other.dataset.entity,default_opacity:op0,option_opacity:op1,after_clear:op2};
ok($('details').textContent.includes(target.rule)&&$('details').textContent.includes(target.label.slice(0,6)),'Expression details show rule and label');
$('clear').click();ok(a.selection===null,'Selection cleared');
// A referenced view is named by its label; its id is everything after the prefix (a view id may hold colons itself).
const refs=G.expressions.filter(e=>e.evidence_ids.some(x=>x.startsWith('references_view:')));facts.references={expressions:refs.length,views_named:0,views_absent:0};
for(const e of refs){d.querySelector('#exprList button[data-expr="'+e.id+'"]').click();for(const x of e.evidence_ids.filter(x=>x.startsWith('references_view:'))){const guess=x.endsWith(':by_elimination'),id=x.slice('references_view:'.length,guess?x.length-':by_elimination'.length:x.length),v=G.views.find(v=>v.id===id);
  if(!v){facts.references.views_absent++;continue}if(!$('details').textContent.includes('指向视图：'+v.label+(guess?'（排除法候选）':'')))throw Error('The details of '+e.id+' do not name the referenced view '+id+' by its label');facts.references.views_named++}}
if(refs.length){$('clear').click();ok(a.selection===null,'Expression details name every referenced view by its label')}else NA.push('expression that references a view');
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
// The complete structure with one kind selected at a time: what is drawn follows the members, and the empty notice never lies
// over drawn structural lines (the outline kinds are drawn by lines only).
facts.complete_by_kind={};for(const kind of a.presentKinds){$('kindNone').click();d.querySelector('[data-kind="'+kind+'"]').click();const s=shownGroups('Complete structure, kind '+kind);facts.complete_by_kind[kind]={groups_shown:s.groups_shown,structural_lines:s.structural_lines,sources_drawn:s.sources_drawn,sources_drawn_by_lines_only:s.sources_drawn_by_lines_only,empty_notice:s.empty_notice};
 if(kind==='visible_contour')ok(s.structural_lines===0||!s.empty_notice,'Only the visible outline selected: no empty notice over drawn structural lines')}
ok(true,'Complete structure, one kind at a time: groups by their claiming members, empty notice only when nothing is drawn');$('kindAll').click();
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
// Masks keep the paper's colour in every mode, and what an entity draws over its mask keeps a colour of its own (readable text).
const PAPER=getComputedStyle(svg0.querySelector(':scope>rect')).fill,MARKED=svg0.querySelector('[data-mask]')!==null,isMask=n=>MARKED?n.hasAttribute('data-mask'):/^(#fff|#ffffff|white)$/i.test(n.getAttribute('fill')||'');
const byEntity=new Map();for(const g of svg0.querySelectorAll(':scope>g[data-entity]')){const k=g.dataset.entity;if(!byEntity.has(k))byEntity.set(k,[]);byEntity.get(k).push(...g.querySelectorAll('[fill],[stroke]'))}
const maskFacts={paper:PAPER,marked:MARKED,masks:0,entities:0,drawn_over:0,recoloured_masks:[],paper_coloured:[]};
for(const [k,els] of byEntity){const ms=els.filter(isMask);if(!ms.length)continue;maskFacts.entities++;for(const m of ms){maskFacts.masks++;if(getComputedStyle(m).fill!==PAPER)maskFacts.recoloured_masks.push(k)}
 for(const n of els.filter(n=>!isMask(n))){const cs=getComputedStyle(n),c=(n.getAttribute('fill')||'none')!=='none'?cs.fill:cs.stroke;maskFacts.drawn_over++;if(c===PAPER)maskFacts.paper_coloured.push(k)}}
ok(!maskFacts.recoloured_masks.length,'Masks keep the paper colour');ok(!maskFacts.paper_coloured.length,'Drawing over masks keeps its own colour');facts.masks=maskFacts;
// Line widths at the screenshot state.
const W=a.widths;ok(W.thick>0&&close(W.thick,2*W.thin),'Thick and thin line widths are 2:1');
const floorNow=()=>{const b=svg0.getBoundingClientRect(),vb=svg0.getAttribute('viewBox').split(/[\s,]+/).map(Number);return Math.max(1,1.6/(W.d*Math.min(b.width/vb[2],b.height/vb[3])))};
ok(close(W.thick,W.d*floorNow())&&close(W.floor,floorNow()),'Drawn widths are d and d/2, raised together only while the thick line would be under 1.6 px on screen');
// d recomputed from the embedded drawing: filled triangles at least twice as long as their base, inside native dimensions (else on
// roots with an arrowhead member); one sixth of their median length (GB/T 4458.4-2003 fig. 9); without any, twice the converter width.
const numbers=t=>((t||'').match(/-?\d+\.?\d*(?:e-?\d+)?/gi)||[]).map(Number),pts=t=>{const q=numbers(t),o=[];for(let i=0;i+1<q.length;i+=2)o.push([q[i],q[i+1]]);return o},far=(p,q)=>Math.hypot(p[0]-q[0],p[1]-q[1]);
const hullOf=P=>{P=[...new Map(P.map(p=>[p[0].toFixed(7)+','+p[1].toFixed(7),p])).values()].sort((p,q)=>p[0]-q[0]||p[1]-q[1]);if(P.length<3)return P;const cr=(o,p,q)=>(p[0]-o[0])*(q[1]-o[1])-(p[1]-o[1])*(q[0]-o[0]),half=Q=>{const h=[];for(const p of Q){while(h.length>=2&&cr(h[h.length-2],h[h.length-1],p)<=1e-9)h.pop();h.push(p)}h.pop();return h};return half(P).concat(half([...P].reverse()))};
const arrowLength=n=>{const h=hullOf(pts(n.getAttribute('d')));if(h.length!==3)return null;let best=null;for(let i=0;i<3;i++){const base=far(h[i],h[(i+1)%3]);if(!best||base<best[0])best=[base,far(h[(i+2)%3],[(h[i][0]+h[(i+1)%3][0])/2,(h[i][1]+h[(i+1)%3][1])/2])]}return best[0]>0&&best[1]/best[0]>=2?best[1]:null};
const PMARKED=pd.querySelector('#canvas>svg [data-mask]')!==null,pMask=n=>PMARKED?n.hasAttribute('data-mask'):/^(#fff|#ffffff|white)$/i.test(n.getAttribute('fill')||''),ARROWS=new Set(G.members.filter(m=>m.member_role==='arrowhead').map(m=>m.source_handle)),inDims=[],looseOnes=[];
for(const g of PRIS){const r=g.dataset.entity,dim=ENT.get(r).kind==='DIMENSION';if(!dim&&!ARROWS.has(r))continue;for(const n of g.querySelectorAll('path')){if(pMask(n))continue;const L=arrowLength(n);if(L)(dim?inDims:looseOnes).push(L)}}
const used=(inDims.length?inDims:looseOnes).sort((x,y)=>x-y),med=used.length?(used.length%2?used[(used.length-1)/2]:(used[used.length/2-1]+used[used.length/2])/2):null,pvb=pd.querySelector('#canvas>svg').getAttribute('viewBox').split(/[\s,]+/).map(Number),conv=Number(pd.querySelector('#canvas>svg [stroke-width]')?.getAttribute('stroke-width'))||pvb[2]/1500;
if(used.length)ok(W.basis==='terminator_length/6'&&W.terminators===used.length&&close(W.d,med/6),'d is one sixth of the median dimension terminator');else ok(W.basis==='2x_converter_width'&&close(W.d,2*conv),'Without a filled terminator d is twice the converter stroke width');
ok(close(W.converter,conv),'The converter stroke width is read from the embedded drawing');
const atShot=strokeWidths('Screenshot state');ok(atShot.thick+atShot.thin+atShot.partial_spans+atShot.structural_thick+atShot.structural_thin>0||!$('empty').hidden,'Every drawn stroke has the width of its member rule');
facts.widths={d:W.d,thick:W.thick,thin:W.thin,floor:W.floor,converter:W.converter,basis:W.basis,terminators:W.terminators,median_terminator_length:W.median_terminator_length,at_screenshot:atShot};
// Member-level display and the empty notice at the screenshot state.
facts.member_display=Object.assign(shownGroups('Screenshot state'),{roots_with_member_identity:IDENT.size});ok(true,'Groups with member identity are shown exactly when a claiming member passes');ok(true,'The empty notice is hidden whenever something is drawn');
// Text backgrounds: the background of a text is painted before the other ink of its own annotation (the whole root for a DIMENSION,
// TEXT or MTEXT root; the block reference that directly holds the text when the background's group names a text member), except ink
// that crosses the text's own glyphs or that a later background of the root must keep hidden. Everything else keeps the converted order.
const NATIVE=JSON.parse(d.getElementById('payload').textContent).native_member_kinds||{},TEXTS=new Set(['TEXT','MTEXT','ATTRIB','ATTDEF']),ONE=new Set(['DIMENSION','TEXT','MTEXT']);
const ownInk=()=>{const L=live(),pos=new Map(L.map((g,i)=>[g,i])),byRoot=new Map(),F={roots:0,masks:0,own_masks:0,roots_in_several_runs:0,own_before:0,in_front:0,kept_behind:0,in_front_touching_mask_box:0},moved=new Set(),P=svg0.createSVGPoint();
 for(const g of [...L].sort((x,y)=>SEQ.get(x)-SEQ.get(y))){const r=g.dataset.entity;if(!byRoot.has(r))byRoot.set(r,[]);byRoot.get(r).push(g)}
 const inked=(n,x,y)=>{P.x=x;P.y=y;return (n.getAttribute('fill')||'none')!=='none'?n.isPointInFill(P):n.isPointInStroke(P)};
 const crossing=(g,glyphs)=>{const B=glyphs.map(n=>{const b=n.getBBox();return [b.x-W.d,b.y-W.d,b.x+b.width+W.d,b.y+b.height+W.d,n]}),hit=(x,y)=>B.some(b=>x>=b[0]&&x<=b[2]&&y>=b[1]&&y<=b[3]&&inked(b[4],x,y));
  for(const n of g.children){if(n.tagName==='polyline'){const q=pts(n.getAttribute('points'));for(let i=0;i+1<q.length;i++){const k=Math.max(1,Math.ceil(far(q[i],q[i+1])/(W.d/8)));for(let j=0;j<=k;j++)if(hit(q[i][0]+(q[i+1][0]-q[i][0])*j/k,q[i][1]+(q[i+1][1]-q[i][1])*j/k))return true}}
   else if(n.tagName==='circle'){if(hit(Number(n.getAttribute('cx')),Number(n.getAttribute('cy'))))return true}
   else{for(const q of pts(n.getAttribute('d')))if(hit(q[0],q[1]))return true;for(const b of B){P.x=(b[0]+b[2])/2;P.y=(b[1]+b[3])/2;if(n.isPointInFill(P))return true}}}return false};
 for(const [r,gs] of byRoot){const masks=gs.filter(g=>[...g.children].some(isMask));if(!masks.length)continue;F.roots++;F.masks+=masks.length;const one=ONE.has(ENT.get(r).kind),runs=!gs.every((g,i)=>!i||SEQ.get(g)===SEQ.get(gs[i-1])+1),lastMask=Math.max(...masks.map(g=>SEQ.get(g)));if(runs)F.roots_in_several_runs++;
  const info=masks.map(M=>{const D=M.dataset.member,own=one||(!!D&&TEXTS.has(NATIVE[D]));if(!own)return {M,own};const parent=D?D.slice(0,D.lastIndexOf('/')):r;
   return {M,own,scope:g=>one||(g.dataset.member?g.dataset.member.startsWith(parent+'/'):parent===r),glyphs:(D?gs.filter(g=>g.dataset.member===D):gs.filter(g=>SEQ.get(g)>lastMask)).flatMap(g=>[...g.children]).filter(n=>!isMask(n))}});
  F.own_masks+=info.filter(m=>m.own).length;const was=gs.map(g=>g.style.display);gs.forEach(g=>g.style.display='');const boxesOf=masks.map(g=>g.getBBox());
  for(const g of gs){if(masks.includes(g))continue;const after=info.filter(m=>SEQ.get(m.M)>SEQ.get(g));if(!after.some(m=>m.own&&m.scope(g)))continue;F.own_before++;
   const free=!runs&&after.every(m=>m.own&&m.scope(g)&&!crossing(g,m.glyphs));
   if(free){if(!after.every(m=>pos.get(g)>pos.get(m.M)))throw Error('A text background hides ink of its own annotation that does not cross the text: '+(g.dataset.member||r));F.in_front++;moved.add(g);const b=g.getBBox();if(boxesOf.some(m=>!(b.x>m.x+m.width||m.x>b.x+b.width||b.y>m.y+m.height||m.y>b.y+b.height)))F.in_front_touching_mask_box++}
   else{if(!after.every(m=>pos.get(g)<pos.get(m.M)))throw Error('Ink that a text background must keep hidden is drawn over it: '+(g.dataset.member||r));F.kept_behind++}}
  gs.forEach((g,i)=>g.style.display=was[i])}
 let prev=-1;for(const g of L){if(moved.has(g))continue;if(SEQ.get(g)<prev)throw Error('A group that is not own ink of a text background left its converted position: '+g.dataset.entity);prev=SEQ.get(g)}
 L.forEach((g,i)=>{if(g.dataset.entity!==PRIS[i].dataset.entity)throw Error('A group left the run of its root: '+g.dataset.entity)});
 return F};
facts.masks_own=ownInk();ok(true,'No mask hides ink of its own annotation except ink crossing its text');ok(true,'Groups keep their converted order except own ink brought in front of its text background');
// Content that never plots: faint in the classified views (class not-plotted, 40 % of the opacity), nothing else is.
const npm=m=>m.evidence_ids.includes('not_plotted'),notPlotted=label=>{const S=pageState(),NPR=G.entities.filter(e=>MS.get(e.id).length>0&&MS.get(e.id).every(npm)).map(e=>e.id);let faint=0,plain=0;
 for(const g of live()){const c=claimers(g),np=!S.raw&&c.length>0&&c.every(npm);if(g.classList.contains('not-plotted')!==np)throw Error(label+': the not-plotted style of '+g.dataset.entity+' does not follow its members');if(g.style.display==='none')continue;
  if(np||plain<60){const base=parseFloat(g.style.getPropertyValue('--op')||'1');if(!close(parseFloat(w.getComputedStyle(g).opacity),(np?.4:1)*base))throw Error(label+': the opacity of '+g.dataset.entity+' is '+w.getComputedStyle(g).opacity);np?faint++:plain++}}
 const shown=NPR.filter(id=>S.root(id).ranges.length>0).length,said=$('status').textContent.includes(shown+' 个不打印层来源');if(!S.raw&&(shown>0)!==said||(!S.raw&&!shown&&$('status').textContent.includes('不打印层来源')))throw Error(label+': the status line does not count '+shown+' not-plotted sources: '+$('status').textContent);
 return {sources:NPR.length,sources_shown:shown,groups_faint:faint}};
facts.not_plotted=notPlotted('Screenshot state');ok(true,'Content that never plots is drawn faint in the classified views and nothing else is');
// The original view is the converter's rendering untouched: every group shown, the converter's own stroke widths and drawing order.
const pageSeq=live().map(g=>SEQ.get(g)),wasRaw=$('original').checked;if(!wasRaw)$('original').click();
{const L=live();let strokes=0;ok(L.length===PRIS.length&&L.every((g,i)=>SEQ.get(g)===i),'Original view keeps the converted drawing order');
 for(const g of L){if(g.style.display==='none')throw Error('Original view hides a group of '+g.dataset.entity);const own=[...PRIS[SEQ.get(g)].querySelectorAll('[stroke-width]')];[...g.querySelectorAll('[stroke-width]')].forEach((n,k)=>{if(n.getAttribute('stroke-width')!==own[k].getAttribute('stroke-width')||!close(px(n),Number(own[k].getAttribute('stroke-width'))))throw Error('Original view changes a converter stroke width: '+g.dataset.entity);strokes++})}
 ok(true,'Original view keeps the converter stroke widths');const np=notPlotted('Original view');ok(np.groups_faint===0,'Original view draws content that never plots like the rest');facts.original_view={strokes,groups:L.length,order_as_converted:true,empty_notice:!$('empty').hidden}}
if(!wasRaw){$('original').click();const back=live().map(g=>SEQ.get(g));ok(back.length===pageSeq.length&&back.every((x,i)=>x===pageSeq[i]),'Leaving the original view restores the page order');facts.widths.after_original=strokeWidths('After leaving the original view');ok(true,'Leaving the original view restores the widths by member')}
facts.not_applicable=NA;document.getElementById('checks').textContent=JSON.stringify({status:'passed',count:checks.length,checks,facts,sourceNodes:baseNodes,roots:N,expressions:G.expressions.length,visibleAtScreenshot:count(),mode});
}catch(e){document.getElementById('checks').textContent=JSON.stringify({status:'failed',error:String(e),stack:e.stack})}};
// Frame f is the page under test; frame p is the same page with scripting disabled (the drawing as embedded).
let loaded=0;for(const id of ['f','p'])document.getElementById(id).onload=()=>{if(++loaded===2)run()};