// One viewer, one set of controls, two source-bound SVGs in drawing coordinates.
const REVIEW=DATA.drafting_review,CORRECTED=REVIEW?.corrected,CG=CORRECTED?.graph;
const correctedEntities=new Map((CG?.source.entities||[]).map(e=>[e.id,e]));
const changedHandles=new Set(REVIEW?.receipt.applied.flatMap(a=>a.result_handles)||[]);
if(CORRECTED){
 const doc=new DOMParser().parseFromString(CORRECTED.svg,'image/svg+xml');
 correctedSvg=document.importNode(doc.documentElement,true);
 // Keep SVG resource identities local when both drawings share this document.
 const ids=new Map([...correctedSvg.querySelectorAll('[id]')].map(n=>[n.id,'corrected-'+n.id]));
 for(const n of correctedSvg.querySelectorAll('*'))for(const attr of [...n.attributes]){
  if(attr.name==='id')n.setAttribute('id',ids.get(attr.value));
  else if(attr.value.includes('url(#'))n.setAttribute(attr.name,attr.value.replace(/url\(#([^)]*)\)/g,(m,id)=>'url(#'+(ids.get(id)||id)+')'));
 }
 correctedSvg.id='correctedSvg';
 correctedSvg.setAttribute('role','img');correctedSvg.setAttribute('aria-label','改正 DXF 重读后的矢量图');
 $('correctedPane').append(correctedSvg);
 const nativeKinds=Object.fromEntries(CG.source.members.filter(m=>m.block_path?.length>1).map(m=>[m.id,m.kind]));
 correctedView=createDrawingView({...CG,entities:CG.source.entities},correctedSvg,nativeKinds);
 bindDrawingInput(correctedSvg,true);
}
function correctedBox(){return correctedSvg.getAttribute('viewBox').split(/\s+/).map(Number)}
function syncComparisonView(){
 if(!correctedSvg)return;
 const ox=Number(correctedSvg.dataset.drawingOriginX),ty=Number(correctedSvg.dataset.drawingTopY);
 correctedSvg.setAttribute('viewBox',[view[0]+originX-ox,view[1]+ty-topY,view[2],view[3]].join(' '));
}
function setDrawingMode(next){
 if(!['source','corrected','compare'].includes(next)||(!correctedSvg&&next!=='source'))return false;
 drawingMode=next;$('canvas').dataset.drawingMode=next;$('sourceFilters').hidden=false;
 svg.style.display=next==='corrected'?'none':'';$('correctedPane').hidden=next==='source';
 $('sourceCaption').hidden=next!=='compare';$('colorLegend').hidden=false;
 for(const [id,value] of [['showSourceDrawing','source'],['showCorrectedDrawing','corrected'],['showComparison','compare']])$(id).setAttribute('aria-pressed',String(next===value));
 syncComparisonView();render();return true;
}
function correctedDrawing(on){return setDrawingMode(on?'corrected':'source')}
function comparisonBoxes(){
 if(!correctedSvg||!correctedSelection)return [];
 const dx=Number(correctedSvg.dataset.drawingOriginX)-originX,dy=topY-Number(correctedSvg.dataset.drawingTopY),boxes=[];
 for(const n of correctedSvg.querySelectorAll('[data-entity]'))if(n.dataset.entity===correctedSelection){const b=n.getBBox();if(b.width||b.height)boxes.push([b.x+dx,b.y+dy,b.x+b.width+dx,b.y+b.height+dy])}
 return boxes;
}
function chooseCorrectedSource(id){
 if(!correctedEntities.has(id))return false;
 correctedSelection=id;selected=entities.has(id)?{type:'source',id}:null;render();return true;
}
function renderComparison(){
 if(!correctedSvg)return;
 syncComparisonView();if(drawingMode==='source')return;
 const selection=correctedSelection?{type:'source',id:correctedSelection}:selected;
 const valid=selection&&(selection.type==='source'?correctedEntities.has(selection.id):selection.type==='line'?correctedView.lineById.has(selection.id):correctedView.exprs.has(selection.id));
 const result=correctedView.draw(valid?selection:null,correctedBox());
 if(drawingMode==='corrected')showDrawingStatus(result);
 for(const n of correctedSvg.querySelectorAll('[data-entity]'))n.classList.toggle('changed',changedHandles.has(n.dataset.entity));
 for(const n of correctedSvg.querySelectorAll('[data-line]'))n.classList.toggle('changed',(correctedView.lineMembers.get(n.dataset.line)||[]).some(m=>changedHandles.has(m.source_handle)));
 if(correctedSelection&&drawingMode!=='source'){
  const e=correctedEntities.get(correctedSelection),members=CG.members.filter(m=>m.source_handle===correctedSelection);
  $('details').innerHTML=`<strong>改正后 · ${escape(PRIM[e.kind]?.[0]||e.kind)} · <code>${escape(e.id)}</code></strong>${e.text?`<p>文字：${escape(e.text)}</p>`:''}<p>来源摘要：<code>${escape(CG.source_sha256)}</code></p><table><thead><tr><th>表达</th><th>角色</th><th>源区间</th></tr></thead><tbody>${members.map(m=>`<tr><td>${escape(CG.expressions.find(x=>x.id===m.expression_id)?.label||m.expression_id)}</td><td>${escape(ROLE[m.member_role]||m.member_role)}</td><td>${m.interval.join(' → ')}</td></tr>`).join('')}</tbody></table>`;
 }
}
async function showDraftingIssue(id,display){
 const item=REVIEW?.issues.find(i=>i.id===id);if(!item)return false;
 setDrawingMode(typeof display==='string'?display:display===true?'corrected':correctedSvg?'compare':'source');
 const hs=item.details.findings?.flatMap(f=>f.source_handles)||item.details.newer_handles||item.source_occurrences.map(h=>h.split('/')[0]);
 const source=hs.find(h=>entities.has(h));if(!source)return false;
 chooseSource(source);
 const applied=REVIEW.receipt.applied.find(a=>a.issue_id===id);
 correctedSelection=applied?.result_handles.find(h=>correctedEntities.has(h))||(correctedEntities.has(source)?source:null);
 render();focus();return true;
}
if(REVIEW){
 $('draftingPanel').hidden=false;
 $('showSourceDrawing').onclick=()=>setDrawingMode('source');
 $('showCorrectedDrawing').onclick=()=>setDrawingMode('corrected');$('showComparison').onclick=()=>setDrawingMode('compare');
 $('showCorrectedDrawing').disabled=!correctedSvg;$('showComparison').disabled=!correctedSvg;$('downloadCorrected').hidden=!correctedSvg;
 $('correctionStatus').textContent=REVIEW.receipt.status==='written'?`已写出 ${REVIEW.receipt.applied.length} 处改正，${REVIEW.receipt.remaining_issue_ids.length} 条保留待核。`:(REVIEW.issues.some(i=>i.confidence==='certain'&&i.classification==='source_deviation_candidate')?'确定条目无可执行的安全操作、未写改正件。':'无确定条目、未写改正件。');
 for(const [kind,name] of [['source_deviation_candidate','原图偏离候选'],['recognition_deviation','识别偏离'],['revision_difference','版本差别']]){
  const items=REVIEW.issues.filter(i=>i.classification===kind),group=document.createElement('details');group.dataset.issueClass=kind;group.open=kind==='source_deviation_candidate';const title=document.createElement('summary');title.textContent=name+' '+items.length+' 条';group.append(title);
  for(const i of items){const row=document.createElement('details');row.dataset.issueId=i.id;const title=document.createElement('summary');title.textContent=({'certain':'确定','likely':'很可能','needs_expert':'要问专家'}[i.confidence]||i.confidence)+' · '+i.rule+' · '+i.location;title.onclick=()=>showDraftingIssue(i.id);row.append(title);const description=document.createElement('p');description.textContent=i.id+'；命中层：'+i.layers.join('、')+'；事实：'+i.observed.join('；')+'；条款：'+i.clause+'；建议：'+i.proposed_correction;row.append(description);const b=document.createElement('button');b.textContent=correctedSvg?'定位对照':'查看原图位置';b.onclick=()=>showDraftingIssue(i.id);row.append(b);group.append(row)}$('draftingIssues').append(group)
 }
 $('exportDraftingIssues').onclick=()=>{const u=URL.createObjectURL(new Blob([REVIEW.text],{type:'text/plain;charset=utf-8'})),a=document.createElement('a');a.href=u;a.download='drafting-issues.txt';a.click();setTimeout(()=>URL.revokeObjectURL(u),0)};
}
