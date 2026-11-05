// Thin installed-SDK calls. Part classification and explicit geometric intent
// remain in the Rust operation and its source-bound task plan.
#include <NXOpen/Features_AdmMoveFaceBuilder.hxx>
#include <NXOpen/Features_FeatureCollection.hxx>
#include <NXOpen/Face.hxx>
#include <NXOpen/FaceDumbRule.hxx>
#include <NXOpen/ScCollector.hxx>
#include <NXOpen/ScRuleFactory.hxx>
#include <NXOpen/Direction.hxx>
#include <NXOpen/DirectionCollection.hxx>
#include <NXOpen/Update.hxx>
#include <NXOpen/BodyCollection.hxx>
#include <NXOpen/NXString.hxx>
#include <NXOpen/Preferences_PartPreferences.hxx>
#include <NXOpen/Preferences_PartModeling.hxx>
#include <uf_mtx.h>
#include <uf_trns.h>

extern "C" __declspec(dllexport) int32_t oe_nx_width_export_precise_v1(
 const OeNxStepExport* o,double tolerance,char* error,uint32_t capacity) noexcept {
 NXOpen::StepCreator* builder=nullptr;
 const int32_t result=guarded([&]{
  if(!o||o->structure_bytes!=sizeof(*o)||!o->input||!o->output||!o->layers||!std::isfinite(tolerance)||tolerance<1e-7||tolerance>0.01)throw std::runtime_error("width_export_options_invalid");
  builder=NXOpen::Session::GetSession()->DexManager()->CreateStepCreator();
  builder->SetExportAs(static_cast<NXOpen::StepCreator::ExportAsOption>(o->application_protocol));
  builder->SetExportFrom(static_cast<NXOpen::StepCreator::ExportFromOption>(o->export_from));
  builder->SetInputFile(o->input);builder->SetOutputFile(o->output);builder->SetLayerMask(o->layers);
  builder->ExportSelectionBlock()->SetSelectionScope(static_cast<NXOpen::ObjectSelector::Scope>(o->selection_scope));
  builder->ObjectTypes()->SetSolids(o->solids!=0);builder->SetFileSaveFlag(o->file_save!=0);builder->SetProcessHoldFlag(o->process_hold!=0);
  builder->SetBsplineTol(tolerance);
  if(std::abs(builder->BsplineTol()-tolerance)>1e-12)throw std::runtime_error("width_export_tolerance_readback_differs");
  // External ExistingPart translation reads a definition file. Persist the
  // requested builder settings into this job's own output namespace and bind it.
  const std::string settings=std::string(o->output)+".settings.def";
  builder->SaveSettings(settings.c_str());
  builder->SetSettingsFile(settings.c_str());
  if(std::abs(builder->BsplineTol()-tolerance)>1e-12)throw std::runtime_error("width_export_saved_tolerance_readback_differs");
  builder->Commit();auto done=builder;builder=nullptr;done->Destroy();
 },error,capacity);
 if(builder){try{builder->Destroy();}catch(...){}}
 return result;
}

extern "C" __declspec(dllexport) int32_t oe_nx_width_translate_body_v1(
 uint32_t p,uint32_t body,const double delta[3],char* error,uint32_t capacity) noexcept {
 return guarded([&]{
  if(!p||!body||!delta)throw std::runtime_error("width_body_translate_input_invalid");
  for(int i=0;i<3;i++)if(!std::isfinite(delta[i])||std::abs(delta[i])>10000)throw std::runtime_error("width_body_translate_delta_invalid");
  auto b=dynamic_cast<NXOpen::Body*>(NXOpen::NXObjectManager::Get(body));
  if(!b||b->OwningPart()->Tag()!=p||UF_ASSEM_is_occurrence(body))throw std::runtime_error("width_body_translate_owner_differs");
  if(UF_ASSEM_set_work_part(p))throw std::runtime_error("width_body_translate_work_part");
  const double rotation[9]={1,0,0,0,1,0,0,0,1};double matrix[16];
  UF_MTX4_initialize(1.0,delta,rotation,matrix);tag_t object=body;
  const int code=UF_MODL_transform_entities(1,&object,matrix);
  if(code)throw std::runtime_error("width_body_translate:"+std::to_string(code));
  if(UF_MODL_update())throw std::runtime_error("width_body_translate_update_failed");
 },error,capacity);
}

// UF char buffers follow UF_ask_codeset, while our evidence is UTF-8.
// Convert through the installed SDK and require a lossless round trip.
extern "C" __declspec(dllexport) int32_t oe_nx_uf_text_utf8_v1(
 const char* input,uint32_t input_bytes,char* output,uint32_t output_capacity,
 int32_t* observed_codeset,char* error,uint32_t error_capacity) noexcept {
 return guarded([&]{
  if(!input||!input_bytes||input_bytes>32768||input[input_bytes-1]!=0||
     !output||!output_capacity||!observed_codeset||strlen(input)+1!=input_bytes)
   throw std::runtime_error("uf_text_conversion_input_invalid");
  UF_codeset_t codeset=UF_CODESET_NOT_INITIALIZED;
  const int code=UF_ask_codeset(&codeset);
  if(code||codeset<0)throw std::runtime_error("uf_text_codeset_query_failed");
  *observed_codeset=static_cast<int32_t>(codeset);
  const auto mode=codeset==UF_CODESET_UTF8?NXOpen::NXString::UTF8:NXOpen::NXString::Locale;
  NXOpen::NXString value(input,mode);
  const char* utf8=value.GetUTF8Text();
  if(!utf8||strlen(utf8)>=output_capacity)throw std::runtime_error("uf_text_utf8_output_invalid");
  NXOpen::NXString check(utf8,NXOpen::NXString::UTF8);
  const char* roundtrip=codeset==UF_CODESET_UTF8?check.GetUTF8Text():check.GetLocaleText();
  if(!roundtrip||strcmp(roundtrip,input)!=0)throw std::runtime_error("uf_text_conversion_not_lossless");
  memcpy(output,utf8,strlen(utf8)+1);
 },error,error_capacity);
}

extern "C" __declspec(dllexport) int32_t oe_nx_owned_body_inventory_v1(
 uint32_t p,uint32_t* tags,uint32_t capacity,uint32_t* count,char* error,uint32_t error_capacity) noexcept {
 return guarded([&]{
  if(!p||!tags||!capacity||capacity>100000||!count)throw std::runtime_error("owned_body_inventory_abi_invalid");
  *count=0;auto bodies=part(p)->Bodies();
  for(auto it=bodies->begin();it!=bodies->end();++it){
   auto body=*it;
   if(!body||*count>=capacity)throw std::runtime_error("owned_body_inventory_budget");
   tags[(*count)++]=body->Tag();
  }
 },error,error_capacity);
}

extern "C" __declspec(dllexport) int32_t oe_nx_width_move_faces_v1(
 uint32_t p,const uint32_t* tags,uint32_t count,const double delta[3],
 uint32_t* feature,char* error,uint32_t capacity) noexcept {
 NXOpen::Features::AdmMoveFaceBuilder* builder=nullptr;
 NXOpen::Session* session=nullptr;
 NXOpen::Session::UndoMarkId mark=static_cast<NXOpen::Session::UndoMarkId>(0);
 bool marked=false;
 const auto result=guarded([&]{
  if(!p||!tags||!count||count>65536||!delta||!feature) throw std::runtime_error("width_move_abi_invalid");
  *feature=0;
  const double distance=std::sqrt(delta[0]*delta[0]+delta[1]*delta[1]+delta[2]*delta[2]);
  if(!std::isfinite(distance)||distance<=0||distance>10000)throw std::runtime_error("width_move_distance_invalid");
  auto owner=part(p);session=NXOpen::Session::GetSession();
  if(UF_ASSEM_set_work_part(p))throw std::runtime_error("width_move_work_part_failed");
  mark=session->SetUndoMark(NXOpen::Session::MarkVisibilityInvisible,"Explicit width end region");marked=true;
  auto modeling=owner->Preferences()->Modeling();
  if(!modeling->GetHistoryMode())modeling->SetHistoryMode();
  if(!modeling->GetHistoryMode())throw std::runtime_error("width_requires_history_mode_for_feature_commit");
  std::vector<NXOpen::Face*> faces;faces.reserve(count);
  for(uint32_t i=0;i<count;i++){
   auto face=dynamic_cast<NXOpen::Face*>(NXOpen::NXObjectManager::Get(tags[i]));
   if(!face||face->OwningPart()->Tag()!=p)throw std::runtime_error("width_face_owner_differs");
   faces.push_back(face);
  }
  builder=owner->Features()->CreateAdmMoveFaceBuilder(nullptr);
  builder->SetMoveOption(NXOpen::Features::AdmMoveFaceBuilder::MoveOptionTypeMoveAndAdapt);
  builder->Motion()->SetOption(NXOpen::GeometricUtilities::ModlMotion::OptionsDistance);
  auto direction=owner->Directions()->CreateDirection(NXOpen::Point3d(0,0,0),NXOpen::Vector3d(delta[0]/distance,delta[1]/distance,delta[2]/distance),NXOpen::SmartObject::UpdateOptionWithinModeling);
  builder->Motion()->SetDistanceVector(direction);builder->Motion()->DistanceValue()->SetValue(distance);
  auto recognition=builder->FaceToMove();
  recognition->SetCoplanarEnabled(false);recognition->SetCoaxialEnabled(false);
  recognition->SetTangentEnabled(false);recognition->SetRigidBodyFaceEnabled(false);
  recognition->SetSymmetricEnabled(false);recognition->SetPatternEnabled(false);
  recognition->SetOffsetEnabled(false);recognition->SetThicknessChainEnabled(false);
  recognition->SetParallelEnabled(false);recognition->SetPerpendicularEnabled(false);
  auto rule=owner->ScRuleFactory()->CreateRuleFaceDumb(faces);
  builder->FaceToMove()->FaceCollector()->ReplaceRules(std::vector<NXOpen::SelectionIntentRule*>{rule},false);
  if(!builder->Validate())throw std::runtime_error("width_move_builder_validation_failed");
  auto committed=builder->Commit();
  auto made=dynamic_cast<NXOpen::Features::Feature*>(committed);
  // Some direct-edit commits expose no Feature object. The Rust owner
  // requires measured face and exact-body changes before any save.
  *feature=made?made->Tag():0;
  auto done=builder;builder=nullptr;done->Destroy();
  if(UF_MODL_update())throw std::runtime_error("width_move_update_failed");
 },error,capacity);
 if(builder){try{builder->Destroy();}catch(...){}}
 if(result!=0&&marked){try{session->UndoToMark(mark,static_cast<const char*>(nullptr));}catch(...){}}
 return result;
}
extern "C" __declspec(dllexport) int32_t oe_nx_width_place_instance_v1(
 uint32_t instance,const double origin[3],const double csys[9],char* error,uint32_t capacity) noexcept {
 return guarded([&]{
  if(!instance||!origin||!csys)throw std::runtime_error("width_place_abi_invalid");
  double o[3],m[9];std::copy(origin,origin+3,o);std::copy(csys,csys+9,m);
  for(double v:o)if(!std::isfinite(v))throw std::runtime_error("width_place_origin_nonfinite");
  for(double v:m)if(!std::isfinite(v))throw std::runtime_error("width_place_matrix_nonfinite");
  const int code=UF_ASSEM_reposition_instance2(instance,o,m);
  if(code)throw std::runtime_error("width_reposition_instance2:"+std::to_string(code));
  const int update=UF_MODL_update();if(update)throw std::runtime_error("width_reposition_update:"+std::to_string(update));
 },error,capacity);
}
