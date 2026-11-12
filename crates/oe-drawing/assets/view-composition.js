// Presentation of explicit graph relations only. Location, labels and bounds never create composition.
function createViewComposition(graph){
 const views=new Map(graph.views.map(v=>[v.id,v]));
 const role=m=>m.view_relation_role||'unknown';
 const targets=m=>role(m)==='component'&&Array.isArray(m.composition_view_ids)?m.composition_view_ids.filter(id=>views.has(id)):[];
 const located=m=>m.located_view_id??m.view_id??'none';
 const componentFor=(m,id)=>role(m)==='component'&&targets(m).includes(id);
 const locatedFor=(m,id)=>views.has(id)&&['located_only','unknown'].includes(role(m))&&located(m)===id;
 const passes=(m,selected,overlay)=>targets(m).some(id=>selected.has(id))||(overlay&&selected.has(located(m))&&locatedFor(m,located(m)));
 const overlayOnly=(m,selected)=>!targets(m).some(id=>selected.has(id))&&selected.has(located(m))&&locatedFor(m,located(m));
 const colorTarget=(m,selected)=>targets(m).find(id=>selected.has(id))||(selected.has(located(m))?located(m):'none');
 const sourceCount=id=>new Set(graph.members.filter(m=>componentFor(m,id)).map(m=>m.source_handle)).size;
 // A source LINE may carry different members on adjacent intervals. Split at their boundaries so that an
 // overlaid dimension span stays faint even when the union covers the whole source. Prefer composition ink
 // where an annotation and a component claim the same span; every member's interval remains in the graph.
 const spans=members=>{
  const ends=[...new Set(members.flatMap(m=>m.interval))].sort((a,b)=>a-b),out=[];
  for(let i=0;i+1<ends.length;i++){
   const a=ends[i],b=ends[i+1];if(b-a<=1e-9)continue;
   const claiming=members.filter(m=>m.interval[0]<b-1e-9&&m.interval[1]>a+1e-9);
   if(claiming.length)out.push({interval:[a,b],members:claiming});
  }
  return out;
 };
 return {role,targets,located,componentFor,locatedFor,passes,overlayOnly,colorTarget,sourceCount,spans};
}
