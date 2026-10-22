use anyhow::{Result,ensure};
use oe_nx_protocol::{*,transfer::SourcePublication,published_payload::*};
use oe_windows_controller::{native_input_copy::{Source as CopySource,Destination},native_payload_copy::prepare,transfer_reconstruct::Target};
use std::{collections::BTreeMap,io::Write};
use serde_json::json;
fn file(path: &str, content: &[u8]) -> FileEntry {
    FileEntry::from_bytes(path, content).unwrap()
}
fn root() -> RootIdentity {
    RootIdentity {
        host_id: "fixture-host".into(),
        volume_id: "volume-1".into(),
        directory_id: "directory-1".into(),
    }
}
fn runtime() -> RuntimeIdentity {
    RuntimeIdentity {
        product: "fixture-native-runtime".into(),
        product_label: "not-an-NX-acceptance".into(),
        architecture: "x64".into(),
        libraries: vec![file("fixture.dll", b"fixture DLL identity")],
    }
}
fn job() -> JobManifest {
    JobManifest {
        capacity: None, auxiliary_capacity: None, schema: JOB_SCHEMA.into(),
        job_id: JobId::new("fixture-job").unwrap(),
        root: root(),
        worker: WorkerManifest {
            schema: WORKER_SCHEMA.into(),
            worker_id: "fixture-worker".into(),
            version: "1".into(),
            target: "x86_64-pc-windows-gnu".into(),
            executable: file("worker/fixture.exe", b"not an executable"),
            runtime_files: vec![file("worker/helper.dll", b"not a DLL")],
            source_files: vec![file("sources/main.rs", b"fixture source")],
            build_recipe: file("build/recipe.json", b"fixture recipe"),
            toolchain: "fixture-toolchain".into(),
            required_runtime: runtime(),
            capabilities: vec![
                Capability {
                    action: "inspect".into(),
                    params_schema: "fixture.inspect/v1".into(),
                },
                Capability {
                    action: "custom.thread_gauge".into(),
                    params_schema: "fixture.gauge/v1".into(),
                },
            ],
        },
        action: "inspect".into(),
        params_schema: "fixture.inspect/v1".into(),
        params: file(
            "params.json",
            br#"{"schema":"fixture.inspect/v1","units":"mm"}"#,
        ),
        inputs: vec![
            file("inputs/主体.prt", b"not a PRT"),
            file("inputs/子装配/轴.prt", b"not another PRT"),
        ],
    }
}

fn capacity()->NativeCapacity{serde_json::from_value(json!({"max_input_bytes":268435456,"max_input_file_bytes":209715200,"max_output_bytes":536870912,"max_output_file_bytes":268435456,"max_input_files":10000,"max_output_files":10000,"max_segment_bytes":104857600})).unwrap()}
fn large()->JobManifest{let mut j=job();j.schema=LARGE_JOB_SCHEMA.into();j.capacity=Some(capacity());j}
#[derive(Default)]struct Sink{data:BTreeMap<String,Vec<u8>>,corrupt:bool,fail_write:bool,max_write:usize}
impl Target for Sink{
 fn identity(&self)->Result<RootIdentity>{Ok(RootIdentity{host_id:"h".into(),volume_id:"v".into(),directory_id:"destination".into()})}
 fn verify_empty(&self)->Result<()>{ensure!(self.data.is_empty(),"occupied");Ok(())}
 fn write_new<F:FnOnce(&mut dyn Write)->Result<()>>(&mut self,file:&FileEntry,write:F)->Result<()>{
  ensure!(!self.data.contains_key(file.path.as_str()),"exists");self.data.insert(file.path.as_str().into(),vec![]);
  struct Stream<'a>{out:&'a mut Vec<u8>,fail:bool,max:&'a mut usize}
  impl Write for Stream<'_>{fn write(&mut self,b:&[u8])->std::io::Result<usize>{*self.max=(*self.max).max(b.len());if self.fail{if self.out.len()>=2{return Err(std::io::Error::other("injected write failure"));}let n=b.len().min(2-self.out.len());self.out.extend_from_slice(&b[..n]);Ok(n)}else{self.out.extend_from_slice(b);Ok(b.len())}}fn flush(&mut self)->std::io::Result<()>{Ok(())}}
  let out=self.data.get_mut(file.path.as_str()).unwrap();write(&mut Stream{out,fail:self.fail_write,max:&mut self.max_write})?;if self.corrupt&&!out.is_empty(){out[0]=b'X';}Ok(())
 }
 fn readback(&mut self,file:&FileEntry)->Result<BlobIdentity>{Ok(BlobIdentity::of(self.data.get(file.path.as_str()).unwrap()))}
 fn verify_complete(&self,files:&[FileEntry])->Result<()>{ensure!(files.len()==self.data.len(),"extra files");for f in files{f.identity.verify(self.data.get(f.path.as_str()).unwrap())?;}Ok(())}
}

impl Destination for Sink{fn root_identity(&self)->Result<RootIdentity>{Ok(RootIdentity{host_id:"h".into(),volume_id:"v".into(),directory_id:"native-root".into()})}}

struct Source{record:SourcePublication,files:Vec<FileEntry>,data:BTreeMap<String,Vec<u8>>,fail_stream:bool,fail_after:bool,reads:usize}
impl CopySource for Source{
 fn publication(&self)->&SourcePublication{&self.record}
 fn files(&self)->&[FileEntry]{&self.files}
 fn verify(&self)->Result<()>{ensure!(!(self.fail_after&&self.reads>=2),"source_changed");Ok(())}
 fn stream(&mut self,file:&FileEntry,max:u64,out:&mut dyn Write)->Result<()>{self.reads+=1;let raw=&self.data[file.path.as_str()];if self.fail_stream{out.write_all(&raw[..raw.len().min(2)])?;anyhow::bail!("copy_interrupted");}for chunk in raw.chunks(max.min(65536) as usize){out.write_all(chunk)?;}Ok(())}
}
fn source(name:&str,data:BTreeMap<String,Vec<u8>>)->Source{
 let files=data.iter().map(|(p,b)|file(p,b)).collect::<Vec<_>>();let root=RootIdentity{host_id:"h".into(),volume_id:"v".into(),directory_id:"source-root".into()};let mut target=root.clone();target.directory_id=format!("{name}-tree");let mut meta=root.clone();meta.directory_id=format!("{name}-publication");
 let record=SourcePublication{schema:"oev2.nx-source-publication/1".into(),source_root:root,job_id:JobId::new(name).unwrap(),plan_digest:Digest::of(name.as_bytes()),output_id:JobId::new(format!("{name}-tree")).unwrap(),publication_id:JobId::new(format!("{name}-publication")).unwrap(),publication_root:meta,target,inventory_digest:record_digest(&files).unwrap(),files:files.len(),bytes:files.iter().map(|f|f.identity.bytes).sum(),state:"published".into(),native_execution_started:false};
 Source{record,files,data,fail_stream:false,fail_after:false,reads:0}
}
fn fixture()->(JobManifest,PublishedPayloadPlan,Vec<Source>){
 let mut j=large();j.root=Sink::default().root_identity().unwrap();
 let data:BTreeMap<String,Vec<u8>>=[("worker/fixture.exe",b"not an executable".as_slice()),("worker/helper.dll",b"not a DLL".as_slice()),("sources/main.rs",b"fixture source".as_slice()),("build/recipe.json",b"fixture recipe".as_slice()),("params.json",br#"{"schema":"fixture.inspect/v1","units":"mm"}"#.as_slice())].into_iter().map(|(p,b)|(p.into(),b.into())).collect();
 let input:BTreeMap<String,Vec<u8>>=[("主体.prt",b"not a PRT".as_slice()),("子装配/轴.prt",b"not another PRT".as_slice())].into_iter().map(|(p,b)|(p.into(),b.into())).collect();
 let sources=vec![source("inputs-source",input),source("program-source",data)];let plan=PublishedPayloadPlan{schema:"oev2.nx-published-payload-plan/1".into(),job_manifest_digest:j.digest().unwrap(),sources:sources.iter().enumerate().map(|(i,s)|PublishedPayloadSource{publication:s.record.clone(),files:s.files.clone(),mount:if i==0{Mount::Inputs}else{Mount::Payload}}).collect()};plan.validate(&j).unwrap();(j,plan,sources)
}
