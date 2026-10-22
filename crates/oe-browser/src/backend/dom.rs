//! Fixed, code-owned page programs. None is an arbitrary CLI evaluate route.
use anyhow::{Result, ensure};
use serde_json::Value;

pub(super) fn origin_guard(expression: &str, origin: &str) -> Result<String> {
    let origin = serde_json::to_string(origin)?;
    Ok(format!(
        "(()=>{{if(location.origin!=={origin})throw new Error('Unexpected target origin');return JSON.stringify(({expression}));}})()"
    ))
}
pub(super) fn apple_script(tab: &str, script: &str) -> Result<String> {
    ensure!(
        tab.parse::<u64>().is_ok_and(|v| v > 0),
        "observed_numeric_tab_id_required"
    );
    Ok(format!(
        "if application \"Google Chrome\" is not running then error \"Chrome is not running\"\ntell application \"Google Chrome\"\n repeat with w in windows\n  repeat with t in tabs of w\n   if (id of t as text) is {} then return execute t javascript {}\n  end repeat\n end repeat\n error \"Observed target tab no longer exists\"\nend tell\n",
        serde_json::to_string(tab)?,
        serde_json::to_string(script)?
    ))
}
pub(super) fn snapshot(source: &str, selectors: &[String]) -> Result<String> {
    let selectors = serde_json::to_string(selectors)?;
    Ok(format!(
        "(()=>{{const p={source};if(!p)return null;const selectors={selectors};{SNAPSHOT_EXTRA}}})()"
    ))
}
const SNAPSHOT_EXTRA: &str = r#"
 const c=window.__oeBrowserSnapshot,extra=[];
 for(const selector of selectors) for(const e of document.querySelectorAll(selector)) {
   if(!e.isConnected||!e.checkVisibility({checkOpacity:true,checkVisibilityCSS:true})||
      e.matches(':disabled')||e.closest('[aria-disabled="true"],[inert]'))continue;
   const r=e.getBoundingClientRect(),x=r.x+r.width/2,y=r.y+r.height/2;
   if(!r.width||!r.height||x<0||y<0||x>=innerWidth||y>=innerHeight)continue;
   if(!c.ids.has(e))c.ids.set(e,c.next++);
   const node=c.ids.get(e);c.nodes.set(node,e);
   if(p.actions.some(a=>a.node===node))continue;
   const label=e.innerText.trim().replace(/\s+/g,' ');if(!label)continue;
   const a={id:'custom_'+node,node,kind:'click',role:'button',label,value:'',rect:{x:r.x,y:r.y,w:r.width,h:r.height}};
   extra.push(a);p.guards[node]=c.guard(e);
 }
 p.actions.push(...extra);p.marker.push(extra.map(({rect,...a})=>a));
 p.selected_values=[...document.querySelectorAll('select')]
   .filter(e=>e.checkVisibility({checkOpacity:true,checkVisibilityCSS:true}))
   .map(e=>({id:e.id,value:e.value,label:[...e.selectedOptions].map(o=>o.label).join(', ')}));
 if(p.selected_values.length)p.text+='\nObserved selected dropdown values: '+JSON.stringify(p.selected_values);
 return p;
"#;

const TARGET: &str = r#"
 const a=p.action,c=window.__oeBrowserSnapshot;
 if(!c||JSON.stringify(c.pageKey())!==JSON.stringify(p.page_key))return {stale:true};
 const e=c.nodes.get(a.node);
 if(!e?.isConnected||JSON.stringify(c.guard(e))!==JSON.stringify(p.guard)||
    e.matches(':disabled')||e.closest('[aria-disabled="true"],[aria-hidden="true"],[inert]')||
    ['password','file','hidden'].includes(e.type)||
    !e.checkVisibility({checkOpacity:true,checkVisibilityCSS:true}))return {stale:true};
 if(a.kind==='fill'&&(e.readOnly||e.getAttribute('aria-readonly')==='true'||typeof p.text!=='string'))return {stale:true};
 const r=e.getBoundingClientRect(),x=r.x+r.width/2,y=r.y+r.height/2;
 if(!r.width||!r.height||x<0||y<0||x>=innerWidth||y>=innerHeight||
    !e.contains(document.elementFromPoint(x,y)))return {stale:true};
"#;
pub(super) fn resolve_target(data: &Value) -> Result<String> {
    Ok(format!(
        "(p=>{{{TARGET}return {{x,y}};}})({})",
        serde_json::to_string(data)?
    ))
}
pub(super) fn apply(data: &Value) -> Result<String> {
    Ok(format!(
        "(p=>{{{APPLY_SCROLL}{TARGET}{APPLY}}})({})",
        serde_json::to_string(data)?
    ))
}
const APPLY_SCROLL: &str = r#"
 if(p.action.kind==='scroll') {
   const c=window.__oeBrowserSnapshot;
   if(!c||JSON.stringify(c.pageKey())!==JSON.stringify(p.page_key))return {stale:true};
   window.scrollBy(0,p.action.delta);return {executed:p.action.id};
 }
"#;
const APPLY: &str = r#"
 if(a.kind==='select') {
   if(e.tagName!=='SELECT'||![...e.options].some(o=>o.value===a.value&&!o.disabled&&!o.closest('optgroup[disabled]')))return {stale:true};
   e.value=a.value;e.dispatchEvent(new Event('input',{bubbles:true}));e.dispatchEvent(new Event('change',{bubbles:true}));
   if(e.value!==a.value)return {outcome_unknown:true};
   return {executed:a.id,readback:e.value,expected:a.value};
 }
 if(a.kind==='fill') {
   const proto=e.tagName==='INPUT'?HTMLInputElement.prototype:e.tagName==='TEXTAREA'?HTMLTextAreaElement.prototype:null;
   if(!proto)return {unsupported:true};
   Object.getOwnPropertyDescriptor(proto,'value').set.call(e,p.text);
   e.dispatchEvent(new Event('input',{bubbles:true}));e.dispatchEvent(new Event('change',{bubbles:true}));
   if(e.value!==p.text)return {outcome_unknown:true};
   return {executed:a.id,readback:e.value,expected:p.text};
 }
 if(a.kind==='click'){e.click();return {executed:a.id};}
 return {unsupported:true};
"#;
pub(super) fn focused(data: &Value) -> Result<String> {
    Ok(format!(
        r#"(p=>{{const c=window.__oeBrowserSnapshot,e=c?.nodes.get(p.action.node);
        return !!e?.isConnected&&document.activeElement===e&&
          performance.timeOrigin===p.page_key[0]&&location.href===p.page_key[1]&&
          !['password','file','hidden'].includes(e.type)&&!e.disabled&&!e.readOnly&&
          e.getAttribute('aria-readonly')!=='true'&&!e.closest('[aria-disabled="true"],[inert]')&&
          e.checkVisibility({{checkOpacity:true,checkVisibilityCSS:true}});}})({})"#,
        serde_json::to_string(data)?
    ))
}
pub(super) fn readback(data: &Value) -> Result<String> {
    Ok(format!(
        r#"(p=>{{const e=window.__oeBrowserSnapshot?.nodes.get(p.action.node);
        if(!e?.isConnected||performance.timeOrigin!==p.page_key[0]||location.href!==p.page_key[1])return null;
        return {{value:'value' in e?e.value:e.isContentEditable?e.innerText:null}};}})({})"#,
        serde_json::to_string(data)?
    ))
}

pub(super) fn settle(action: &Value, origin: &str) -> Result<String> {
    Ok(format!(
        r#"(()=>{{if(location.origin!=={})throw new Error('Unexpected target origin');
      const action={};return new Promise(resolve=>{{
       const field=window.__oeBrowserSnapshot?.nodes.get(action.node);
       const autocomplete=action.kind==='fill'&&field?.getAttribute('role')==='combobox';
       let frames=0,stopped=false;const finish=()=>{{stopped=true;resolve(true)}};
       setTimeout(finish,autocomplete?200:50);
       const ready=()=>{{if(stopped)return;
        const ids=(field?.getAttribute('aria-controls')||field?.getAttribute('aria-owns')||'').split(/\s+/).filter(Boolean);
        const roots=ids.length?ids.map(id=>document.getElementById(id)).filter(Boolean):[document];
        const options=roots.flatMap(root=>[...root.querySelectorAll('[role="option"]')]);
        if(++frames>=2&&(!autocomplete||options.some(e=>{{const r=e.getBoundingClientRect();return r.width&&r.height&&r.bottom>0&&r.top<innerHeight&&e.checkVisibility({{checkOpacity:true,checkVisibilityCSS:true}})}})))finish();
        else requestAnimationFrame(ready);
       }};requestAnimationFrame(ready);
      }});}})()"#,
        serde_json::to_string(origin)?,
        serde_json::to_string(action)?
    ))
}

pub(super) const SUPPLIER: &str = r#"(() => {
 const visible=e=>e?.checkVisibility({checkOpacity:true,checkVisibilityCSS:true});
 const d=[...document.querySelectorAll('.new_cadDl')].find(visible),f=d?.querySelector('#cad_format');
 const text=document.body.innerText,part=new URL(location.href).searchParams.get('HissuCode');
 const displayed=text.match(/型号[：:]\s*([A-Za-z0-9_.-]+)\s*复制\s*重新选型/);
 const exact=part&&text.split(/[^A-Za-z0-9_.-]+/).includes(part);
 const specAt=text.search(/产品概述|规格概述|规格表|基本信息/);
 return {source_url:location.href,part,popup_visible:!!d,format_value:f?.value??null,
   configured:!!part&&((displayed&&displayed[1]===part)||(text.includes('型号已生成')&&exact)),
   displayed_part:displayed?.[1]??null,
   configuration_marker_present:text.includes('型号已生成'),exact_part_token_present:!!exact,
   format_options:f?[...f.options].map(o=>({value:o.value,label:o.label,disabled:o.disabled||!!o.closest('optgroup[disabled]')})):[],
   dialog_text:d?.innerText??'',specification_excerpt:specAt>=0?text.slice(specAt,specAt+2500):null,
   observed_download_url:d?.querySelector('#cad_download_link')?.href??null,
   documents:[...document.querySelectorAll('a[href]')].filter(visible)
     .filter(e=>/\.(pdf|zip)(?:[?#]|$)/i.test(e.href)&&e.innerText.trim())
     .map(e=>({label:e.innerText.trim(),url:e.href}))};
})()"#;

pub(super) const ACCOUNT: &str = r#"(() => {
 const visible=e=>e?.checkVisibility({checkOpacity:true,checkVisibilityCSS:true});
 const value=id=>{const e=document.getElementById(id);return visible(e)?e.innerText.trim():null};
 const inputs=[...document.querySelectorAll('input')].filter(visible),pw=inputs.filter(e=>e.type==='password');
 const ids=inputs.filter(e=>['text','email','tel'].includes(e.type)&&!/search|搜索/i.test([e.name,e.id,e.placeholder].join(' ')));
 const autofilled=e=>{try{return e.matches(':autofill')||e.matches(':-webkit-autofill')}catch{return false}};
 const challenge=[...document.querySelectorAll('iframe[title*="reCAPTCHA"],iframe[title*="hCaptcha"],input[autocomplete="one-time-code"],input[name*="captcha"]')].some(visible);
 return {url:location.origin+location.pathname,title:document.title,ready:document.readyState,
   account_fields:{username:value('userName'),email:value('userEmail')},
   login_form_visible:pw.length===1&&/\/login\/?$/.test(location.pathname),
   autofill_ready:pw.length===1&&autofilled(pw[0])&&ids.length===1&&autofilled(ids[0]),
   autofill_signal:'css_pseudoclass_only',password_values_read:false,
   challenge_visible:challenge,access_denied:document.title==='Access Denied'};
})()"#;
