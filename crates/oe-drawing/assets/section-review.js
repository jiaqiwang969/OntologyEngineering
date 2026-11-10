// Section evidence is a read-only overlay. It never replaces or repairs source geometry.
const sectionReview=(()=>{
 const typeName={superimposed:'重合断面',removed:'移出断面',unknown:'断面类型未知'};
 const originName={pre_existing:'剖前已有',cut_formed:'剖切形成',suppressed:'剖后应消失',unknown:'来源未知'};
 const value=(e,p)=>e.evidence_ids.find(x=>x.startsWith(p))?.slice(p.length);
 const regions=G.expressions.filter(e=>value(e,'cross_section_type:'));
 const spans=[];
 for(const m of G.members)for(const ev of m.evidence_ids){if(!ev.startsWith('section_boundary:'))continue;
  try{const s=JSON.parse(ev.slice('section_boundary:'.length));if(Array.isArray(s.points)&&s.points.length===2)spans.push({...s,member:m})}catch{/* old or unsupported evidence stays in the evidence list */}}
 const layer=document.createElementNS(NS,'g');layer.id='section-boundaries';layer.style.pointerEvents='none';svg.insertBefore(layer,overlay);
 const list=$('sectionList');
 for(const e of regions){const b=document.createElement('button');b.className='peer';b.dataset.sectionRegion=e.id;
  b.textContent=`${viewLabel.get(vkey(e))||''} · ${typeName[value(e,'cross_section_type:')]||'未知'} · ${spans.filter(s=>s.region===e.id).length} 段边界`;
  b.onclick=()=>chooseExpr(e.id,true);list.append(b)}
 if(!regions.length)list.textContent='没有已读出的剖面区域。';
 function describe(e){const t=value(e,'cross_section_type:');if(!t)return '';
  const ss=spans.filter(s=>s.region===e.id),axes=ss.filter(s=>s.carrier==='center_line').length;
  const partial=value(e,'section_boundary_geometry:')==='partial_or_unavailable';
  return `<p class="section-summary"><strong>${typeName[t]||escape(t)}</strong>${t==='unknown'?'：配置或线宽依据不足，概念为候选':''}；已核边界 ${ss.length} 段${axes?`，其中 ${axes} 段由中心线兼作`:''}。${partial?'曲边或散画剖面边界尚未全部确认。':''}</p>`;
 }
 function memberOrigin(m){const o=value(m,'section_origin:');return o?originName[o]||escape(o):'—'}
 function render(){layer.replaceChildren();const raw=$('original').checked;
  $('sectionDisplayNote').textContent=raw?'原图对照：保留转换器原笔画，边界辅助高亮暂时关闭。':completeOn()?'完整结构：沿用已有局部剖视隐藏规则；边界高亮只标当前可见的源区间，不补线。':'原图表达：保留剖面与内部原线；边界高亮只标已有源区间，不补线。';
  if(raw||!$('sectionEdges').checked)return;
  for(const s of spans){if(!passes(s.member))continue;
   const n=document.createElementNS(NS,s.arc?'path':'line'),p=s.points;
   if(s.arc){const [cx,cy,r,a,sw]=s.arc,pt=t=>`${cx+r*Math.cos(t)-originX},${topY-cy-r*Math.sin(t)}`,mid=a+sw/2;
    n.setAttribute('d',`M${pt(a)} A${r},${r} 0 0 0 ${pt(mid)} A${r},${r} 0 0 0 ${pt(a+sw)}`);n.setAttribute('fill','none');
   }else{n.setAttribute('x1',p[0][0]-originX);n.setAttribute('y1',topY-p[0][1]);n.setAttribute('x2',p[1][0]-originX);n.setAttribute('y2',topY-p[1][1]);}
   n.setAttribute('stroke',s.carrier==='center_line'?'#b45309':'#087f8c');n.setAttribute('stroke-width',D*2.4);n.setAttribute('stroke-dasharray',`${D*3} ${D*2}`);n.setAttribute('opacity','.7');
   n.dataset.sectionBoundary=s.region;n.dataset.sectionMember=s.member.id;n.dataset.sectionInterval=JSON.stringify(s.interval);layer.append(n);
  }
 }
 $('sectionEdges').onchange=render;
 return {render,describe,memberOrigin,regions,spans};
})();
