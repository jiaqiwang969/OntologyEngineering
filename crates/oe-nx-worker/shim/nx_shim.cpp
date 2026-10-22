// Narrow vendor ABI only. Rust owns jobs, input/output policy and all judgments.
#include "nx_shim.h"
#include <stddef.h>
#include <string.h>
#include <exception>
#include <stdexcept>
#include <uf.h>
#include <uf_part.h>
#include <uf_ps.h>
#include <uf_modl_utilities.h>
#include <type_traits>
#include <cmath>
#include <limits>
#include <uf_modl_types.h>
#include <uf_object_types.h>
#include <uf_obj.h>
#include <uf_modl.h>
#include <uf_modl_primitives.h>
#include <uf_assem.h>
#include <uf_draw.h>
#include <uf_view.h>
#include <uf_drf.h>
#include <uf_eval.h>
#include <uf_csys.h>
#include <uf_so.h>
#include <uf_curve.h>
#include <NXOpen/Annotations_Dimension.hxx>
#include <NXOpen/Session.hxx>
#include <NXOpen/NXException.hxx>
#include <NXOpen/NXObjectManager.hxx>
#include <NXOpen/Part.hxx>
#include <NXOpen/Expression.hxx>
#include <NXOpen/ExpressionCollection.hxx>
#include <NXOpen/UnitCollection.hxx>
#include <NXOpen/Unit.hxx>
#include <NXOpen/Update.hxx>
#include <NXOpen/ErrorList.hxx>
#include <NXOpen/ErrorInfo.hxx>
#include <NXOpen/DexManager.hxx>
#include <NXOpen/StepCreator.hxx>
#include <NXOpen/Step214Importer.hxx>
#include <NXOpen/ObjectSelector.hxx>
#include <NXOpen/ObjectTypeSelector.hxx>
#include <NXOpen/PartCollection.hxx>
#include <NXOpen/LoadOptions.hxx>
#include <NXOpen/Assemblies_ComponentAssembly.hxx>
#include <NXOpen/Assemblies_Component.hxx>
#include <NXOpen/Assemblies_AssemblyManager.hxx>
#include <NXOpen/Assemblies_AddComponentBuilder.hxx>
#include <NXOpen/PartLoadStatus.hxx>
#include <NXOpen/ReferenceSet.hxx>
#include <NXOpen/Body.hxx>
#include <NXOpen/Facet_FacetedBody.hxx>
#include <algorithm>
#include <memory>
#include <NXOpen/WCS.hxx>
#include <NXOpen/Annotations_AppendedText.hxx>
#include <NXOpen/PlotManager.hxx>
#include <NXOpen/PrintPDFBuilder.hxx>
#include <NXOpen/PlotSourceBuilder.hxx>

static_assert(sizeof(tag_t)==4, "tag_t differs from reviewed x64 SDK");
static_assert(sizeof(OeNxComponentCopyRequest1)==32 && offsetof(OeNxComponentCopyRequest1,name)==24,
    "component-copy request POD differs");
static_assert(sizeof(OeNxComponentCopyObservation1)==328
    && offsetof(OeNxComponentCopyObservation1,returned_count)==32
    && offsetof(OeNxComponentCopyObservation1,returned_components)==44,
    "component-copy outcome POD differs");
static_assert(std::is_same_v<decltype(&UF_ASSEM_rename_instance),int(*)(tag_t,const char*)>,
    "instance rename signature differs");
static_assert(UF_solid_type==70 && UF_solid_body_subtype==0,
    "assembly object body type/subtype differs from installed SDK");
static_assert(UF_solid_swept_body_subtype==1 && UF_solid_face_subtype==2
    && UF_solid_edge_subtype==3 && UF_solid_silhouette_subtype==4
    && UF_solid_foreign_surf_subtype==5 && UF_solid_frame_subtype==6,
    "reviewed solid-type non-body subtype list differs from installed SDK");
static_assert(sizeof(logical)==1, "logical differs from reviewed x64 SDK");
static_assert(sizeof(int)==4 && sizeof(double)==8 && sizeof(void*)==8, "x64 ABI required");
static_assert(sizeof(UF_PART_load_state_t)==4,"assembly load-state enum ABI differs");
static_assert(std::is_same_v<decltype(&UF_ask_load_state_of_part),int(*)(tag_t,UF_PART_load_state_t*)>,"assembly load-state signature differs");
static_assert(std::is_same_v<decltype(&UF_ASSEM_ask_all_part_occ_children),int(*)(tag_t,tag_t**)>,"all-children signature differs");
static_assert(std::is_same_v<decltype(&UF_ASSEM_ask_inst_of_part_occ),tag_t(*)(tag_t)>,"definition-instance signature differs");
static_assert(std::is_same_v<decltype(&UF_ASSEM_ask_comp_position),int(*)(tag_t,tag_t,double(*)[4])>,"root-absolute transform signature differs");
static_assert(std::is_same_v<decltype(&UF_ASSEM_find_occurrence),tag_t(*)(tag_t,tag_t)>,"body occurrence finder signature differs");
static_assert(std::is_same_v<decltype(&UF_ASSEM_ask_part_occurrence),tag_t(*)(tag_t)>,"containing part occurrence signature differs");
static_assert(std::is_same_v<decltype(&UF_ASSEM_is_occurrence),logical(*)(tag_t)>,"occurrence logical ABI differs");
static_assert(std::is_same_v<decltype(&UF_PART_save_all),int(*)(int*,tag_t**,int**)>,"assembly save-all signature differs");
static_assert(std::is_same_v<decltype(&UF_PART_ask_nth_part),tag_t(*)(int)>,"loaded part iteration signature differs");
static_assert(std::is_same_v<decltype(&UF_MODL_ask_mass_props_3d),int(*)(tag_t*,int,int,int,double,int,double*,double*,double*)>,"native geometrical volume signature differs");
static_assert(std::is_same_v<decltype(&UF_ASSEM_add_part_to_assembly2),int(*)(tag_t,const char*,const char*,const char*,double*,double*,int,tag_t*,UF_PART_load_status_t*)>,"entire-part fixture add signature differs");
static_assert(sizeof(UF_PART_load_status_t)==24,"entire-add raw status storage differs");
static_assert(sizeof(OeNxUfEntireAdd)==136 && offsetof(OeNxUfEntireAdd,prototype)==24
    && offsetof(OeNxUfEntireAdd,origin)==40 && offsetof(OeNxUfEntireAdd,csys)==64,"entire-add request layout differs");
static_assert(sizeof(OeNxUfEntireAddCall)==80 && offsetof(OeNxUfEntireAddCall,before)==32
    && offsetof(OeNxUfEntireAddCall,after)==56,"entire-add observation layout differs");
// Compile the exact new Rust UF signatures against the installed vendor headers.
static_assert(std::is_same_v<decltype(&UF_PS_import_data), int(*)(char*, uf_list_p_t*)>, "Parasolid import ABI differs");
static_assert(std::is_same_v<decltype(&UF_MODL_ask_bounding_box_exact), int(*)(tag_t, tag_t, double*, double(*)[3], double*)>, "accurate bounding box ABI differs");
static_assert(std::is_same_v<decltype(&UF_MODL_ask_face_data),int(*)(tag_t,int*,double*,double*,double*,double*,double*,int*)>,"face data ABI differs");
static_assert(std::is_same_v<decltype(&UF_MODL_create_cyl1),int(*)(UF_FEATURE_SIGN,double*,char*,char*,double*,tag_t*)>,"cylinder ABI differs");
static_assert(std::is_same_v<decltype(&UF_ASSEM_set_work_part),int(*)(tag_t)>,"work part ABI differs");
static_assert(std::is_same_v<decltype(&UF_ASSEM_ask_work_part),tag_t(*)()>,"work part query ABI differs");
static_assert(std::is_same_v<decltype(&UF_DRAW_ask_drawings),int(*)(int*,tag_t**)>,"drawing list ABI differs");
static_assert(std::is_same_v<decltype(&UF_DRAW_ask_views),int(*)(tag_t,int*,tag_t**)>,"drawing views ABI differs");
static_assert(std::is_same_v<decltype(&UF_DRAW_is_object_out_of_date),int(*)(tag_t,logical*)>,"stale logical ABI differs");
static_assert(std::is_same_v<decltype(&UF_DRF_is_annotation_retained),int(*)(tag_t,logical*)>,"retained logical ABI differs");
static_assert(std::is_same_v<decltype(&UF_DRF_ask_associativity_data),int(*)(tag_t,int*,UF_DRF_object_assoc_data_p_t*)>,"annotation associativity ABI differs");
static_assert(std::is_same_v<decltype(&UF_MODL_ask_body_edges),int(*)(tag_t,uf_list_p_t*)>,"body edge list ABI differs");
static_assert(std::is_same_v<decltype(&UF_DRAW_create_drawing),int(*)(const char*,const UF_DRAW_info_p_t,tag_p_t)>,"create drawing ABI differs");
static_assert(std::is_same_v<decltype(&UF_DRAW_import_view),int(*)(tag_t,tag_t,double*,UF_DRAW_view_info_t*,tag_t*)>,"import drawing view ABI differs");
static_assert(std::is_same_v<decltype(&UF_DRAW_ask_drawing_info),int(*)(tag_t,UF_DRAW_info_t*)>,"drawing info ABI differs");
static_assert(std::is_same_v<decltype(&UF_VIEW_ask_tag_of_view_name),int(*)(const char*,tag_t*)>,"named view ABI differs");
static_assert(std::is_same_v<decltype(&UF_VIEW_ask_rotation),int(*)(tag_t,double*)>,"view rotation ABI differs");
static_assert(std::is_same_v<decltype(&UF_VIEW_ask_type),int(*)(tag_t,UF_VIEW_type_t*,UF_VIEW_subtype_t*)>,"view type ABI differs");
static_assert(sizeof(UF_VIEW_type_t)==4 && sizeof(UF_VIEW_subtype_t)==4,"view enum ABI differs");
static_assert(std::is_same_v<decltype(&UF_DRAW_ask_view_scale),int(*)(tag_t,tag_t*,double*)>,"drawing view scale ABI differs");
static_assert(std::is_same_v<decltype(&UF_DRAW_ask_drawing_ref_pt),int(*)(tag_t,double*)>,"drawing view reference ABI differs");
static_assert(std::is_same_v<decltype(&UF_DRAW_set_drawing_ref_pt),int(*)(tag_t,tag_t,const double*)>,"drawing reference setter ABI differs");
static_assert(std::is_same_v<decltype(&UF_DRAW_move_view),int(*)(tag_t,const double*)>,"drawing view mover ABI differs");
static_assert(std::is_same_v<decltype(&UF_DRAW_ask_view_anchor),int(*)(tag_t,tag_t*)>,"drawing view anchor query ABI differs");
static_assert(std::is_same_v<decltype(&UF_DRAW_set_view_anchor),int(*)(tag_t,tag_t)>,"drawing view anchor setter ABI differs");
static_assert(std::is_same_v<decltype(&UF_DRAW_set_drawing_info),int(*)(tag_t,UF_DRAW_info_p_t)>,"drawing info setter ABI differs");
static_assert(std::is_same_v<decltype(&UF_CURVE_ask_point_data),int(*)(tag_t,double*)>,"layout point data ABI differs");
static_assert(std::is_same_v<decltype(&UF_VIEW_ask_center),int(*)(tag_t,double*)>,"model view center ABI differs");
static_assert(std::is_same_v<decltype(&UF_VIEW_set_center),int(*)(tag_t,double*)>,"model view center setter ABI differs");
static_assert(std::is_same_v<decltype(&UF_VIEW_map_model_to_drawing),int(*)(tag_t,double*,double*)>,"model point to drawing mapping ABI differs");
static_assert(std::is_same_v<decltype(&UF_PART_set_display_part),int(*)(tag_t)>,"display part setter ABI differs");
static_assert(std::is_same_v<decltype(&UF_PART_ask_display_part),tag_t(*)()>,"display part identity ABI differs");
static_assert(UF_VIEW_IS_NOT_DISPLAYED==1170056,"view display error code differs");
static_assert(std::is_same_v<decltype(&UF_DRAW_set_display_state),int(*)(int)>,"drawing display state setter ABI differs");
static_assert(std::is_same_v<decltype(&UF_DRAW_ask_display_state),int(*)(int*)>,"drawing display state query ABI differs");
static_assert(std::is_same_v<decltype(&UF_DRAW_ask_current_drawing),int(*)(tag_t*)>,"current drawing query ABI differs");
static_assert(std::is_same_v<decltype(&UF_VIEW_ask_work_view),int(*)(tag_t*)>,"work view query ABI differs");
static_assert(UF_VIEW_DRAWING_SHEET_TYPE==2,"drawing sheet view kind differs from installed SDK");
static_assert(std::is_same_v<decltype(&UF_SO_is_so),int(*)(tag_t,logical*)>,"smart layout point query ABI differs");
static_assert(std::is_same_v<decltype(&UF_SO_create_scalar_double_dim),int(*)(tag_t,UF_SO_update_option_t,double,UF_SO_scalar_dim_option_t,tag_t*)>,"dimensioned layout scalar ABI differs");
static_assert(std::is_same_v<decltype(&UF_SO_create_point_3_scalars),int(*)(tag_t,UF_SO_update_option_t,const tag_t*,tag_t*)>,"smart layout point ABI differs");
static_assert(std::is_same_v<decltype(&UF_SO_set_visibility_option),int(*)(tag_t,UF_SO_visibility_option_t)>,"layout point visibility ABI differs");
static_assert(UF_DRAW_A0==1 && UF_DRAW_A4==5 && UF_DRAW_A==1 && UF_DRAW_J==8,"standard drawing size codes differ");
static_assert(std::is_same_v<decltype(&UF_DRAW_ask_view_borders),int(*)(tag_t,double*)>,"drawing view borders ABI differs");
static_assert(std::is_same_v<decltype(&UF_DRAW_define_bound_by_objects),int(*)(tag_t,int,tag_t*)>,"drawing boundary definition ABI differs");
static_assert(std::is_same_v<decltype(&UF_DRAW_ask_bound_by_objects),int(*)(tag_t,int*,tag_t**)>,"drawing boundary inventory ABI differs");
static_assert(std::is_same_v<decltype(&UF_DRAW_ask_boundary_type),int(*)(tag_t,UF_DRAW_boundary_type_t*)>,"drawing boundary type ABI differs");
static_assert(sizeof(UF_DRAW_boundary_type_t)==4 && UF_DRAW_BOUND_BY_OBJECTS_TYPE==4,"drawing boundary enum ABI differs");
static_assert(std::is_same_v<decltype(&UF_DRAW_get_view_model_view_part),int(*)(tag_t,char*)>,"drawing view model part ABI differs");
static_assert(std::is_same_v<decltype(&UF_DRAW_ask_drawing_of_view),int(*)(tag_t,tag_t*)>,"view drawing owner ABI differs");
static_assert(std::is_same_v<decltype(&UF_VIEW_ask_visible_objects),int(*)(tag_t,int*,tag_t**,int*,tag_t**)>,"view visible objects ABI differs");
static int32_t fail(int32_t code, const char *message, char *buffer, uint32_t capacity) noexcept {
    if(buffer && capacity){const size_t n=message?strlen(message):0;const size_t copy=n<capacity-1?n:capacity-1;if(copy)memcpy(buffer,message,copy);buffer[copy]=0;}
    return code==0?-90001:code;
}
template<class F> static int32_t guarded(F body,char *error,uint32_t capacity) noexcept {
    if(error && capacity)error[0]=0;
    try{body();return 0;}
    catch(const NXOpen::NXException &e){return fail(e.ErrorCode(),e.what(),error,capacity);}
    catch(const std::exception &e){return fail(-90002,e.what(),error,capacity);}
    catch(...){return fail(-90003,"vendor_unknown_exception",error,capacity);}
}
static NXOpen::Part *part(uint32_t tag){
    // Installed standalone example initializes the C++ view after UF_initialize.
    if(!NXOpen::Session::GetSession())throw std::runtime_error("vendor_cpp_session_missing");
    auto p=dynamic_cast<NXOpen::Part*>(NXOpen::NXObjectManager::Get(tag));
    if(!p)throw std::runtime_error("vendor_tag_is_not_part");return p;
}
int32_t oe_nx_drafting_abi(int32_t constants[8],uint32_t count) noexcept {
    if(!constants || count!=8u)return -90004;
    const int32_t values[8]={1,UF_DRAW_CUSTOM_SIZE,UF_DRAW_FIRST_ANGLE_PROJECTION,
        UF_DRAW_THIRD_ANGLE_PROJECTION,UF_view_type,UF_VIEW_MODEL_TYPE,
        UF_VIEW_DRAWING_MEMBER_TYPE,UF_VIEW_IMPORTED_SUBTYPE};
    std::copy(values,values+8,constants);return 0;
}
int32_t oe_nx_create_drafting_sheet(const char *name,int32_t units,double width,double height,
    double scale,int32_t projection,uint32_t *drawing,int32_t *query_code,char *error,uint32_t error_capacity) noexcept {
    if(!name || !drawing || !query_code)return -90004;
    *drawing=0;*query_code=-1;
    return guarded([&]{
        UF_DRAW_info_t info={};info.size_state=UF_DRAW_CUSTOM_SIZE;
        info.size.custom_size[0]=height;info.size.custom_size[1]=width;
        info.drawing_scale=scale;info.units=units;
        info.projection_angle=static_cast<UF_DRAW_projection_angle_t>(projection);
        *query_code=UF_DRAW_create_drawing(name,&info,drawing);
        if(*query_code!=0)throw std::runtime_error("native_drawing_create_failed");
    },error,error_capacity);
}
int32_t oe_nx_set_drafting_sheet_info_v1(uint32_t drawing,int32_t units,double width,double height,
    double scale,int32_t projection,int32_t codes[2],char *error,uint32_t capacity) noexcept {
    if(!drawing || !codes)return -90004;
    codes[0]=-1;codes[1]=-1;
    return guarded([&]{
        UF_DRAW_info_t info={};codes[0]=UF_DRAW_ask_drawing_info(drawing,&info);
        if(codes[0]!=0)throw std::runtime_error("native_drawing_resize_info_query_failed");
        info.size_state=UF_DRAW_CUSTOM_SIZE;info.size.custom_size[0]=height;info.size.custom_size[1]=width;
        info.drawing_scale=scale;info.units=units;info.projection_angle=static_cast<UF_DRAW_projection_angle_t>(projection);
        codes[1]=UF_DRAW_set_drawing_info(drawing,&info);
        if(codes[1]!=0)throw std::runtime_error("native_drawing_resize_failed");
    },error,capacity);
}
int32_t oe_nx_import_drafting_view(uint32_t drawing,uint32_t model_view,const char *model_name,
    double scale,const double reference[2],uint32_t *view,int32_t *query_code,char *error,uint32_t error_capacity) noexcept {
    if(!drawing || !model_view || !model_name || !reference || !view || !query_code)return -90004;
    *view=0;*query_code=-1;
    return guarded([&]{
        const size_t length=strlen(model_name);
        if(!length || length>=UF_DRAW_MAX_LABEL_LEN)throw std::runtime_error("native_drawing_model_name_bound");
        UF_DRAW_view_info_t info={};UF_DRAW_initialize_view_info(&info);
        info.view_scale=scale;info.use_ref_pt=false;info.inherit_boundary=false;
        // A model-space annotation is not silently promoted to a sheet dimension.
        info.transfer_annotation=false;info.inherit_pmi=false;
        memcpy(info.model_name,model_name,length+1);
        double point[2]={reference[0],reference[1]};
        *query_code=UF_DRAW_import_view(drawing,model_view,point,&info,view);
        if(*query_code!=0)throw std::runtime_error("native_drawing_import_view_failed");
    },error,error_capacity);
}
// Separate v1 entry points retain the original import ABI. Rust computes the
// geometry-only layout center; this boundary only calls the typed vendor APIs.
int32_t oe_nx_create_layout_anchor_v1(uint32_t object_in_part,const double coordinates[3],
    uint32_t scalar_tags[3],uint32_t *anchor,int32_t codes[5],char *error,uint32_t capacity) noexcept {
    if(!object_in_part || !coordinates || !scalar_tags || !anchor || !codes)return -90004;
    std::fill(scalar_tags,scalar_tags+3,0u);*anchor=0;std::fill(codes,codes+5,-1);
    return guarded([&]{
        for(size_t i=0;i<3;++i){
            if(!std::isfinite(coordinates[i]))throw std::runtime_error("layout_anchor_coordinate_nonfinite");
            codes[i]=UF_SO_create_scalar_double_dim(object_in_part,UF_SO_update_within_modeling,
                coordinates[i],UF_SO_scalar_dimensionality_length,&scalar_tags[i]);
            if(codes[i]!=0 || !scalar_tags[i])throw std::runtime_error("layout_anchor_scalar_creation_failed");
        }
        codes[3]=UF_SO_create_point_3_scalars(object_in_part,UF_SO_update_after_modeling,scalar_tags,anchor);
        if(codes[3]!=0 || !*anchor)throw std::runtime_error("layout_anchor_point_creation_failed");
        // Only the newly created layout helper point is invisible. No existing
        // model entity, annotation or complete solid-bound inventory is hidden.
        codes[4]=UF_SO_set_visibility_option(*anchor,UF_SO_invisible);
        if(codes[4]!=0)throw std::runtime_error("layout_anchor_visibility_failed");
    },error,capacity);
}
int32_t oe_nx_import_drafting_view_with_anchor_v1(uint32_t drawing,uint32_t model_view,const char *model_name,
    double scale,const double reference[2],uint32_t anchor,uint32_t *view,int32_t *query_code,
    char *error,uint32_t capacity) noexcept {
    if(!drawing || !model_view || !model_name || !reference || !anchor || !view || !query_code)return -90004;
    *view=0;*query_code=-1;
    return guarded([&]{
        const size_t length=strlen(model_name);
        if(!length || length>=UF_DRAW_MAX_LABEL_LEN)throw std::runtime_error("native_drawing_model_name_bound");
        UF_DRAW_view_info_t info={};UF_DRAW_initialize_view_info(&info);
        info.view_scale=scale;info.anchor_point=anchor;info.use_ref_pt=false;info.inherit_boundary=false;
        info.transfer_annotation=false;info.inherit_pmi=false;
        memcpy(info.model_name,model_name,length+1);
        double point[2]={reference[0],reference[1]};
        *query_code=UF_DRAW_import_view(drawing,model_view,point,&info,view);
        if(*query_code!=0)throw std::runtime_error("native_drawing_anchored_import_failed");
    },error,capacity);
}
int32_t oe_nx_drafting_sheet_info(uint32_t drawing,int32_t raw[4],double values[3],
    char *error,uint32_t error_capacity) noexcept {
    if(!drawing || !raw || !values)return -90004;
    std::fill(raw,raw+4,-1);std::fill(values,values+3,std::numeric_limits<double>::quiet_NaN());
    return guarded([&]{
        UF_DRAW_info_t info={};raw[0]=UF_DRAW_ask_drawing_info(drawing,&info);
        if(raw[0]!=0)throw std::runtime_error("native_drawing_info_failed");
        raw[1]=info.units;raw[2]=info.size_state;raw[3]=info.projection_angle;
        values[2]=info.drawing_scale;
        if(info.size_state==UF_DRAW_CUSTOM_SIZE){values[0]=info.size.custom_size[1];values[1]=info.size.custom_size[0];}
    },error,error_capacity);
}
int32_t oe_nx_drafting_sheet_info_v2(uint32_t drawing,int32_t raw[6],double values[3],
    char *error,uint32_t error_capacity) noexcept {
    if(!drawing || !raw || !values)return -90004;
    std::fill(raw,raw+6,-1);std::fill(values,values+3,std::numeric_limits<double>::quiet_NaN());
    return guarded([&]{
        UF_DRAW_info_t info={};raw[0]=UF_DRAW_ask_drawing_info(drawing,&info);
        if(raw[0]!=0)throw std::runtime_error("native_drawing_info_failed");
        raw[1]=info.units;raw[2]=info.size_state;raw[3]=info.projection_angle;
        values[2]=info.drawing_scale;
        if(info.size_state==UF_DRAW_CUSTOM_SIZE){raw[5]=0;values[0]=info.size.custom_size[1];values[1]=info.size.custom_size[0];}
        else if(info.size_state==UF_DRAW_METRIC_SIZE){raw[5]=1;raw[4]=info.size.metric_size_code;}
        else if(info.size_state==UF_DRAW_ENGLISH_SIZE){raw[5]=2;raw[4]=info.size.english_size_code;}
    },error,error_capacity);
}
int32_t oe_nx_extension_abi(OeNxExtensionAbi *out,uint32_t bytes) noexcept {
    if(!out || bytes!=sizeof(*out))return -90004;
    *out={OE_NX_EXTENSION_VERSION,sizeof(*out),UF_NEGATIVE,UF_dimension_type,UF_drafting_entity_type};return 0;
}
int32_t oe_nx_annotation_boxes(uint32_t annotation,OeNxTextBox *out,uint32_t capacity,uint32_t *count,char *error,uint32_t error_capacity) noexcept {
    return guarded([&]{
        if(!out || !count || !capacity || capacity>64u)throw std::runtime_error("annotation_box_abi_invalid");
        *count=0;int number=0;UF_DRF_draft_aid_text_info_t *info=nullptr;
        const int code=UF_DRF_ask_draft_aid_text_info(annotation,&number,&info);
        if(code!=0)throw std::runtime_error("annotation_text_query_failed");
        struct Guard {int number;UF_DRF_draft_aid_text_info_t *info;~Guard(){if(info)UF_DRF_free_text(number,&info);}} guard{number,info};
        if(number<0 || static_cast<uint32_t>(number)>capacity || (number && !info))throw std::runtime_error("annotation_text_count_bound");
        for(int i=0;i<number;++i){
            const auto &v=info[i];out[i]={{v.origin[0],v.origin[1],v.origin[2]},v.length,v.height,v.angle,v.distance};
        }
        *count=static_cast<uint32_t>(number);
    },error,error_capacity);
}
int32_t oe_nx_association_abi(OeNxAssociationAbi *out,uint32_t bytes) noexcept {
    if(!out || bytes!=sizeof(*out))return -90004;
    *out={1u,sizeof(*out),sizeof(OeNxAssociation),
        {offsetof(OeNxAssociation,object_1),offsetof(OeNxAssociation,object_2),offsetof(OeNxAssociation,view),
         offsetof(OeNxAssociation,point_type),offsetof(OeNxAssociation,line_type),offsetof(OeNxAssociation,position)},
        UF_solid_face_subtype,UF_solid_edge_subtype};return 0;
}
int32_t oe_nx_annotation_associations(uint32_t annotation,OeNxAssociation *out,uint32_t capacity,uint32_t *count,
    int32_t *query_code,char *error,uint32_t error_capacity) noexcept {
    if(!out || !count || !query_code || !capacity || capacity>256u)return -90004;
    *count=0;*query_code=-1;
    return guarded([&]{
        int number=0;UF_DRF_object_assoc_data_p_t associations=nullptr;
        *query_code=UF_DRF_ask_associativity_data(annotation,&number,&associations);
        if(*query_code!=0)throw std::runtime_error("annotation_association_query_failed");
        // A successful normal return owns exactly this array. No UF object or
        // allocation crosses the boundary; unused option fields are not read.
        struct Guard {UF_DRF_object_assoc_data_p_t data;~Guard(){if(data)UF_free(data);}} guard{associations};
        if(number<0 || static_cast<uint32_t>(number)>capacity || (number>0 && !associations))
            throw std::runtime_error("annotation_association_count_invalid");
        for(int i=0;i<number;++i){const auto &v=associations[i];
            out[i]={v.assoc_object_1,v.assoc_object_2,v.object_view,v.point_type,v.line_type,
                {v.assoc_point[0],v.assoc_point[1],v.assoc_point[2]}};
        }
        *count=static_cast<uint32_t>(number);
    },error,error_capacity);
}
int32_t oe_nx_model_abi(OeNxModelAbi *out,uint32_t bytes) noexcept {
    if(!out || bytes!=sizeof(*out))return -90004;
    *out={1u,sizeof(*out),sizeof(OeNxModelEdge),
        {offsetof(OeNxModelEdge,initialize_code),offsetof(OeNxModelEdge,is_arc_code),offsetof(OeNxModelEdge,arc_code),
         offsetof(OeNxModelEdge,free_code),offsetof(OeNxModelEdge,is_arc),offsetof(OeNxModelEdge,is_periodic),
         offsetof(OeNxModelEdge,limits),offsetof(OeNxModelEdge,radius),offsetof(OeNxModelEdge,center),
         offsetof(OeNxModelEdge,x_axis),offsetof(OeNxModelEdge,y_axis)}};
    return 0;
}
int32_t oe_nx_model_edge(uint32_t edge,OeNxModelEdge *out,uint32_t bytes,char *error,uint32_t error_capacity) noexcept {
    if(!out || bytes!=sizeof(*out) || !edge)return -90004;
    *out={};out->initialize_code=out->is_arc_code=out->arc_code=out->free_code=-1;
    return guarded([&]{
        UF_EVAL_p_t evaluator=nullptr;
        out->initialize_code=UF_EVAL_initialize(edge,&evaluator);
        struct Guard {UF_EVAL_p_t evaluator;OeNxModelEdge *out;~Guard(){if(evaluator)out->free_code=UF_EVAL_free(evaluator);}} guard{evaluator,out};
        if(out->initialize_code!=0 || !evaluator)throw std::runtime_error("edge_evaluator_initialize_failed");
        logical is_arc=false;out->is_arc_code=UF_EVAL_is_arc(evaluator,&is_arc);
        if(out->is_arc_code!=0)throw std::runtime_error("edge_arc_kind_query_failed");
        out->is_arc=is_arc?1u:0u;
        if(is_arc){
            UF_EVAL_arc_t arc={};out->arc_code=UF_EVAL_ask_arc(evaluator,&arc);
            if(out->arc_code!=0)throw std::runtime_error("edge_arc_data_query_failed");
            out->is_periodic=arc.is_periodic?1u:0u;
            out->limits[0]=arc.limits[0];out->limits[1]=arc.limits[1];out->radius=arc.radius;
            for(size_t i=0;i<3u;++i){out->center[i]=arc.center[i];out->x_axis[i]=arc.x_axis[i];out->y_axis[i]=arc.y_axis[i];}
        }
    },error,error_capacity);
}
int32_t oe_nx_dimension_value(uint32_t dimension,double *out,char *error,uint32_t error_capacity) noexcept {
    if(!out || !dimension)return -90004;
    *out=std::numeric_limits<double>::quiet_NaN();
    return guarded([&]{
        auto d=dynamic_cast<NXOpen::Annotations::Dimension*>(NXOpen::NXObjectManager::Get(dimension));
        if(!d)throw std::runtime_error("annotation_is_not_dimension");
        *out=d->ComputedSize();
    },error,error_capacity);
}
int32_t oe_nx_parallel_reference(uint32_t edge1,uint32_t edge2,const double plane_origin[3],
    const double plane_matrix[9],const double text_origin_wcs[3],uint32_t *dimension,int32_t codes[6],
    char *error,uint32_t error_capacity) noexcept {
    if(!edge1 || !edge2 || edge1==edge2 || !plane_origin || !plane_matrix || !text_origin_wcs || !dimension || !codes)return -90004;
    *dimension=0;std::fill(codes,codes+6,-1);
    return guarded([&]{
        tag_t previous=0,matrix=0,csys=0;
        codes[0]=UF_CSYS_ask_wcs(&previous);
        if(codes[0]!=0 || !previous)throw std::runtime_error("reference_wcs_query_failed");
        struct Restore {tag_t previous;int32_t *codes;~Restore(){codes[5]=UF_CSYS_set_wcs(previous);}} restore{previous,codes};
        codes[1]=UF_CSYS_create_matrix(plane_matrix,&matrix);
        if(codes[1]!=0 || !matrix)throw std::runtime_error("reference_plane_matrix_failed");
        codes[2]=UF_CSYS_create_csys(plane_origin,matrix,&csys);
        if(codes[2]!=0 || !csys)throw std::runtime_error("reference_plane_csys_failed");
        codes[3]=UF_CSYS_set_wcs(csys);
        if(codes[3]!=0)throw std::runtime_error("reference_plane_set_failed");
        UF_DRF_object_t first={},second={};
        first.object_tag=edge1;second.object_tag=edge2;
        first.object_assoc_type=UF_DRF_arc_center;second.object_assoc_type=UF_DRF_arc_center;
        UF_DRF_text_t text={};
        double origin[3]={text_origin_wcs[0],text_origin_wcs[1],text_origin_wcs[2]};
        codes[4]=UF_DRF_create_parallel_dim(&first,&second,&text,origin,dimension);
        if(codes[4]!=0)throw std::runtime_error("reference_dimension_creation_failed");
        if(!*dimension)throw std::runtime_error("reference_dimension_null");
    },error,error_capacity);
}
int32_t oe_nx_abi(OeNxAbi *out,uint32_t bytes) noexcept {
    if(!out||bytes!=sizeof(OeNxAbi))return -90004;
    *out={OE_NX_ABI_VERSION,sizeof(OeNxAbi),sizeof(tag_t),sizeof(logical),sizeof(UF_PART_load_status_t),
        offsetof(UF_PART_load_status_t,failed),offsetof(UF_PART_load_status_t,user_abort),offsetof(UF_PART_load_status_t,n_parts),
        offsetof(UF_PART_load_status_t,file_names),offsetof(UF_PART_load_status_t,statuses),
        UF_PART_METRIC,UF_PART_ENGLISH,UF_solid_type,UF_MODL_SOLID_BODY,UF_MODL_SHEET_BODY,UF_MODL_PLANAR_FACE,UF_NULLSIGN,
        NXOpen::StepCreator::ExportAsOptionAp214,NXOpen::StepCreator::ExportFromOptionExistingPart,
        NXOpen::ObjectSelector::ScopeEntirePart,NXOpen::Step214Importer::ImportToOptionNewPart,
        NXOpen::LoadOptions::LoadMethodFromDirectory,NXOpen::LoadOptions::LoadComponentsAll,
        MAX_FSPEC_BUFSIZE,UF_OBJ_NAME_BUFSIZE,UF_CFI_MAX_FILE_NAME_BUFSIZE,
        OE_NX_JT_VERSION,sizeof(OeNxBodyJtObservation),
        {offsetof(OeNxBodyJtObservation,version),offsetof(OeNxBodyJtObservation,structure_bytes),
         offsetof(OeNxBodyJtObservation,owner),offsetof(OeNxBodyJtObservation,solid),
         offsetof(OeNxBodyJtObservation,solid_owner),offsetof(OeNxBodyJtObservation,call_entered),
         offsetof(OeNxBodyJtObservation,call_returned),offsetof(OeNxBodyJtObservation,has_jt),
         offsetof(OeNxBodyJtObservation,jt),offsetof(OeNxBodyJtObservation,up_to_date),
         offsetof(OeNxBodyJtObservation,complete),offsetof(OeNxBodyJtObservation,exception_code)},
        UF_faceted_model_type,UF_faceted_model_jt_body_subtype};
    return 0;
}
int32_t oe_nx_load_options(int32_t method,int32_t components,uint32_t partial,uint32_t generate_missing,char *error,uint32_t capacity) noexcept {
    return guarded([&]{auto options=NXOpen::Session::GetSession()->Parts()->LoadOptions();
        options->SetComponentLoadMethod(static_cast<NXOpen::LoadOptions::LoadMethod>(method));
        options->SetComponentsToLoad(static_cast<NXOpen::LoadOptions::LoadComponents>(components));
        options->SetUsePartialLoading(partial!=0);options->SetGenerateMissingPartFamilyMembers(generate_missing!=0);},error,capacity);
}
int32_t oe_nx_expression_create(uint32_t p,const char *formula,const char *unit,uint32_t *out,char *error,uint32_t capacity) noexcept {
    return guarded([&]{if(!formula||!unit||!out)throw std::runtime_error("null_expression_argument");
        auto owner=part(p);auto units=owner->UnitCollection()->FindObject(unit);
        auto expression=owner->Expressions()->CreateWithUnits(formula,units);*out=expression->Tag();},error,capacity);
}
int32_t oe_nx_expression_edit_update(uint32_t p,uint32_t e,const char *formula,int32_t *errors,char *error,uint32_t capacity) noexcept {
    return guarded([&]{if(!formula||!errors)throw std::runtime_error("null_update_argument");
        auto owner=part(p);auto expression=dynamic_cast<NXOpen::Expression*>(NXOpen::NXObjectManager::Get(e));
        if(!expression)throw std::runtime_error("vendor_tag_is_not_expression");
        auto session=NXOpen::Session::GetSession();
        auto mark=session->SetUndoMark(NXOpen::Session::MarkVisibilityInvisible,"Rust requested expression update");
        owner->Expressions()->Edit(expression,formula);*errors=session->UpdateManager()->DoUpdate(mark);},error,capacity);
}
int32_t oe_nx_step_export(const OeNxStepExport *o,char *error,uint32_t capacity) noexcept {
    NXOpen::StepCreator *builder=nullptr;
    const int32_t result=guarded([&]{
        if(!o||o->structure_bytes!=sizeof(*o)||!o->input||!o->output||!o->layers)throw std::runtime_error("invalid_export_abi");
        builder=NXOpen::Session::GetSession()->DexManager()->CreateStepCreator();
        builder->SetExportAs(static_cast<NXOpen::StepCreator::ExportAsOption>(o->application_protocol));
        builder->SetExportFrom(static_cast<NXOpen::StepCreator::ExportFromOption>(o->export_from));
        builder->SetInputFile(o->input);builder->SetOutputFile(o->output);builder->SetLayerMask(o->layers);
        builder->ExportSelectionBlock()->SetSelectionScope(static_cast<NXOpen::ObjectSelector::Scope>(o->selection_scope));
        builder->ObjectTypes()->SetSolids(o->solids!=0);builder->SetFileSaveFlag(o->file_save!=0);builder->SetProcessHoldFlag(o->process_hold!=0);
        builder->Commit();auto done=builder;builder=nullptr;done->Destroy();
    },error,capacity);
    if(builder){try{builder->Destroy();}catch(...){}}
    return result;
}
int32_t oe_nx_step_import(const OeNxStepImport *o,char *error,uint32_t capacity) noexcept {
    NXOpen::Step214Importer *builder=nullptr;
    const int32_t result=guarded([&]{
        if(!o||o->structure_bytes!=sizeof(*o)||!o->input||!o->output)throw std::runtime_error("invalid_import_abi");
        builder=NXOpen::Session::GetSession()->DexManager()->CreateStep214Importer();
        builder->SetInputFile(o->input);builder->SetOutputFile(o->output);
        builder->SetImportTo(static_cast<NXOpen::Step214Importer::ImportToOption>(o->import_to));
        builder->ObjectTypes()->SetSolids(o->solids!=0);builder->SetFileOpenFlag(o->file_open!=0);builder->SetProcessHoldFlag(o->process_hold!=0);
        builder->Commit();auto done=builder;builder=nullptr;done->Destroy();
    },error,capacity);
    if(builder){try{builder->Destroy();}catch(...){}}
    return result;
}
int32_t oe_nx_assembly_abi_v1(int32_t values[3],uint32_t count) noexcept {
    if(!values||count!=3)return -90004;
    values[0]=1;values[1]=sizeof(UF_PART_load_state_t);values[2]=UF_PART_fully_loaded;
    return 0;
}
int32_t oe_nx_assembly_load_diagnostic_v1(int32_t values[6],uint32_t count,
    char *error,uint32_t capacity) noexcept {
    if(!values||count!=6u||!error||capacity==0||capacity>4096)return -90512;
    for(size_t i=0;i<6;i++)values[i]=-1;
    return guarded([&]{
        auto options=NXOpen::Session::GetSession()->Parts()->LoadOptions();
        if(!options)throw std::runtime_error("assembly_load_options_missing");
        values[0]=1;values[1]=static_cast<int32_t>(options->ComponentLoadMethod());
        values[2]=static_cast<int32_t>(options->ComponentsToLoad());
        values[3]=options->UsePartialLoading()?1:0;
        values[4]=options->UseLightweightRepresentations()?1:0;
        values[5]=options->GenerateMissingPartFamilyMembers()?1:0;
    },error,capacity);
}
int32_t oe_nx_step_assembly_import_v1(const OeNxStepImport *o,uint32_t *flatten_after_set,
    char *error,uint32_t capacity) noexcept {
    NXOpen::Step214Importer *builder=nullptr;
    const int32_t result=guarded([&]{
        if(!o||o->structure_bytes!=sizeof(*o)||!o->input||!o->output||!flatten_after_set)
            throw std::runtime_error("invalid_assembly_import_abi");
        builder=NXOpen::Session::GetSession()->DexManager()->CreateStep214Importer();
        builder->SetInputFile(o->input);builder->SetOutputFile(o->output);
        builder->SetImportTo(static_cast<NXOpen::Step214Importer::ImportToOption>(o->import_to));
        builder->ObjectTypes()->SetSolids(o->solids!=0);
        builder->SetFlattenAssembly(false);*flatten_after_set=builder->FlattenAssembly()?1u:0u;
        builder->SetFileOpenFlag(o->file_open!=0);builder->SetProcessHoldFlag(o->process_hold!=0);
        builder->Commit();auto done=builder;builder=nullptr;done->Destroy();
    },error,capacity);
    if(builder){try{builder->Destroy();}catch(...){}}
    return result;
}

static_assert(sizeof(OeNxAssemAdd)==128,"assembly input layout");
static_assert(sizeof(OeNxOpenAddObservation)==65776,"NXOpen add result layout");
static_assert(sizeof(OeNxRefsetObservation)==240,"refset result layout");
static_assert(sizeof(OeNxBodyJtObservation)==48,"body JT result layout");
namespace {
struct Region { const void *pointer; size_t bytes; };
static bool region_valid(Region r) noexcept {
    const uintptr_t p=reinterpret_cast<uintptr_t>(r.pointer);
    return p!=0 && r.bytes!=0 && r.bytes<=(std::numeric_limits<uintptr_t>::max)()-p;
}
static bool overlap(Region a,Region b) noexcept {
    const uintptr_t p=reinterpret_cast<uintptr_t>(a.pointer),q=reinterpret_cast<uintptr_t>(b.pointer);
    return p<q+b.bytes && q<p+a.bytes;
}
static bool ascii_argument(const char *p,uint32_t n,uint32_t limit) noexcept {
    if(!p||n==0||n>limit||p[n-1]!=0)return false;
    for(uint32_t i=0;i+1<n;i++)if(static_cast<unsigned char>(p[i])<32||static_cast<unsigned char>(p[i])>126)return false;
    return true;
}
static void copied_text(const NXOpen::NXString &s,char *out,size_t capacity) {
    const char *text=s.GetUTF8Text();if(!text)throw std::runtime_error("null_vendor_utf8_text");
    size_t n=0;while(n<capacity && text[n])++n;
    if(n==capacity)throw std::runtime_error("vendor_utf8_text_exceeds_bound");
    memcpy(out,text,n+1);
}
static bool named(const NXOpen::NXString &s) {
    char name[64];copied_text(s,name,sizeof(name));return strcmp(name,OE_NX_CONTROLLED_REFSET)==0;
}
static void refset_members(NXOpen::ReferenceSet *set,OeNxRefsetObservation *out) {
    const auto members=set->AskAllDirectMembers();
    if(members.size()>OE_NX_STATUS_LIMIT)throw std::runtime_error("reference_set_member_bound");
    out->member_count=static_cast<uint32_t>(members.size());
    for(size_t i=0;i<members.size();++i){if(!members[i]||members[i]->Tag()==NULL_TAG)throw std::runtime_error("reference_set_null_member");out->members[i]=members[i]->Tag();}
}
}
int32_t oe_nx_uf_entire_add_v1(const OeNxUfEntireAdd *request,void *storage,uint32_t status_bytes,
    OeNxUfEntireAddCall *out,uint32_t out_bytes,char *error,uint32_t capacity) noexcept {
    if(!request||!storage||!out||!error||status_bytes!=sizeof(UF_PART_load_status_t)
        ||out_bytes!=sizeof(*out)||capacity==0||capacity>4096
        ||reinterpret_cast<uintptr_t>(request)%alignof(OeNxUfEntireAdd)
        ||reinterpret_cast<uintptr_t>(storage)%alignof(UF_PART_load_status_t)
        ||reinterpret_cast<uintptr_t>(out)%alignof(OeNxUfEntireAddCall))return -90510;
    const Region regions[]={{request,sizeof(*request)},{storage,status_bytes},{out,out_bytes},{error,capacity}};
    for(size_t i=0;i<4;i++){
        if(!region_valid(regions[i]))return -90510;
        for(size_t j=i+1;j<4;j++)if(overlap(regions[i],regions[j]))return -90510;
    }
    memset(out,0,sizeof(*out));out->version=1;out->structure_bytes=sizeof(*out);
    out->status_bytes=sizeof(UF_PART_load_status_t);out->tag_bytes=sizeof(tag_t);
    out->logical_bytes=sizeof(logical);out->native_code=-1;error[0]=0;
    if(request->version!=1||request->structure_bytes!=sizeof(*request)||request->parent==NULL_TAG
        ||request->layer!=-1
        ||!ascii_argument(request->prototype,request->prototype_bytes,MAX_FSPEC_NCHARS+1u)
        ||!ascii_argument(request->name,request->name_bytes,UF_OBJ_NAME_NCHARS+1u))
        return fail(-90511,"invalid_entire_add_request",error,capacity);
    for(double value:request->origin)if(!std::isfinite(value))return fail(-90511,"nonfinite_entire_add_origin",error,capacity);
    for(double value:request->csys)if(!std::isfinite(value))return fail(-90511,"nonfinite_entire_add_matrix",error,capacity);
    auto status=static_cast<UF_PART_load_status_t*>(storage);
    // Include padding in the explicit zero initialization. The same fixed UF
    // operation is called once; this boundary adds no load-option setter.
    memset(status,0,sizeof(*status));memcpy(out->before,status,sizeof(*status));
    const int32_t wrapper=guarded([&]{
        double origin[3],csys[9];memcpy(origin,request->origin,sizeof(origin));memcpy(csys,request->csys,sizeof(csys));
        tag_t instance=NULL_TAG;
        out->native_code=UF_ASSEM_add_part_to_assembly2(request->parent,request->prototype,nullptr,
            request->name,origin,csys,-1,&instance,status);
        out->instance=instance;out->returned=1;
    },error,capacity);
    memcpy(out->after,status,sizeof(*status));
    return wrapper;
}
int32_t oe_nx_body_jt(uint32_t owner,uint32_t solid,
    OeNxBodyJtObservation *out,uint32_t bytes,char *error,uint32_t capacity) noexcept {
    if(!out||bytes!=sizeof(*out)||reinterpret_cast<uintptr_t>(out)%alignof(OeNxBodyJtObservation)||
       !error||capacity==0||capacity>4096||!region_valid({out,bytes})||!region_valid({error,capacity})||
       overlap({out,bytes},{error,capacity}))return -90410;
    memset(out,0,sizeof(*out));out->version=OE_NX_JT_VERSION;out->structure_bytes=sizeof(*out);
    out->owner=owner;out->solid=solid;error[0]=0;
    if(owner==NULL_TAG||solid==NULL_TAG)return fail(-90411,"invalid_body_jt_request",error,capacity);
    const int32_t code=guarded([&]{
        auto owner_part=part(owner);
        auto body=dynamic_cast<NXOpen::Body*>(NXOpen::NXObjectManager::Get(solid));
        if(!body||!body->OwningPart())throw std::runtime_error("jt_source_is_not_owned_body");
        out->solid_owner=body->OwningPart()->Tag();
        if(out->solid_owner!=owner_part->Tag())throw std::runtime_error("jt_source_owner_differs");
        // Exactly one read-only association query; neither object is owned here.
        NXOpen::Facet::FacetedBody *facet=nullptr;bool current=false;
        out->call_entered=1;
        body->GetFacetedBody(&facet,&current);
        out->call_returned=1;
        out->has_jt=facet?1u:0u;out->up_to_date=current?1u:0u;
        if(facet){out->jt=facet->Tag();if(out->jt==NULL_TAG)throw std::runtime_error("jt_object_has_null_tag");}
        out->complete=1;
    },error,capacity);
    if(code!=0)out->exception_code=code;
    return code;
}
int32_t oe_nx_controlled_refset(uint32_t owner,uint32_t body,uint32_t mode,
    OeNxRefsetObservation *out,uint32_t bytes,char *error,uint32_t capacity) noexcept {
    if(!out||bytes!=sizeof(*out)||reinterpret_cast<uintptr_t>(out)%alignof(OeNxRefsetObservation)||
       !error||capacity==0||capacity>4096||!region_valid({out,bytes})||!region_valid({error,capacity})||overlap({out,bytes},{error,capacity}))return -90210;
    memset(out,0,sizeof(*out));out->version=OE_NX_BOUNDARY_VERSION;out->structure_bytes=sizeof(*out);
    out->operation=mode;out->owner=owner;out->requested_body=body;error[0]=0;
    if(owner==NULL_TAG||(mode!=1&&mode!=2)||(mode==1&&body==NULL_TAG))return fail(-90211,"invalid_refset_request",error,capacity);
    return guarded([&]{
        auto p=part(owner);auto sets=p->GetAllReferenceSets();
        if(sets.size()>OE_NX_STATUS_LIMIT)throw std::runtime_error("reference_set_inventory_bound");
        out->total_sets=static_cast<uint32_t>(sets.size());NXOpen::ReferenceSet *found=nullptr;
        for(auto set:sets){if(!set)throw std::runtime_error("null_reference_set");if(named(set->Name())){if(found)throw std::runtime_error("duplicate_controlled_reference_set");found=set;}}
        if(mode==1){
            if(found)throw std::runtime_error("controlled_reference_set_already_exists");
            auto object=dynamic_cast<NXOpen::NXObject*>(NXOpen::NXObjectManager::Get(body));
            if(!object||!object->OwningPart()||object->OwningPart()->Tag()!=owner)throw std::runtime_error("reference_set_body_owner_differs");
            out->mutation_started=1;found=p->CreateReferenceSet();
            if(!found||found->Tag()==NULL_TAG)throw std::runtime_error("reference_set_creation_null");
            out->reference_set=found->Tag();found->SetName(NXOpen::NXString(OE_NX_CONTROLLED_REFSET,NXOpen::NXString::UTF8));
            copied_text(found->Name(),out->name,sizeof(out->name));
            if(strcmp(out->name,OE_NX_CONTROLLED_REFSET))throw std::runtime_error("reference_set_name_readback_differs");
            refset_members(found,out);out->initial_count=out->member_count;
            if(out->initial_count)throw std::runtime_error("new_reference_set_not_empty");
            found->AddObjectsToReferenceSet(std::vector<NXOpen::NXObject*>{object});
        }
        if(found){out->found=1;out->reference_set=found->Tag();copied_text(found->Name(),out->name,sizeof(out->name));refset_members(found,out);}
        out->complete=1;
    },error,capacity);
}
int32_t oe_nxopen_add_component(const OeNxAssemAdd *o,uint32_t options_bytes,
    OeNxOpenAddObservation *out,uint32_t bytes,char *error,uint32_t capacity) noexcept {
    if(!o||options_bytes!=sizeof(*o)||reinterpret_cast<uintptr_t>(o)%alignof(OeNxAssemAdd)||
       !out||bytes!=sizeof(*out)||reinterpret_cast<uintptr_t>(out)%alignof(OeNxOpenAddObservation)||!error||capacity==0||capacity>4096)return -90310;
    const Region outputs[]={{out,bytes},{error,capacity}};
    const Region inputs[]={{o,options_bytes},{o->prototype,o->prototype_bytes},{o->refset,o->refset_bytes},{o->name,o->name_bytes}};
    for(const auto &r:outputs)if(!region_valid(r))return -90310;
    for(const auto &r:inputs)if(!region_valid(r))return -90310;
    if(overlap(outputs[0],outputs[1]))return -90310;
    for(const auto &a:outputs)for(const auto &b:inputs)if(overlap(a,b))return -90310;
    memset(out,0,sizeof(*out));out->version=OE_NX_BOUNDARY_VERSION;out->structure_bytes=sizeof(*out);out->unloaded_count=-1;error[0]=0;
    if(o->version!=1||o->structure_bytes!=sizeof(*o)||o->reserved||o->parent==NULL_TAG||o->layer!=-1||
       !ascii_argument(o->prototype,o->prototype_bytes,MAX_FSPEC_BUFSIZE)||o->prototype_bytes==1||
       !ascii_argument(o->refset,o->refset_bytes,64)||strcmp(o->refset,OE_NX_CONTROLLED_REFSET)||
       !ascii_argument(o->name,o->name_bytes,UF_OBJ_NAME_BUFSIZE)||o->name_bytes==1)return fail(-90311,"invalid_controlled_assembly_argument",error,capacity);
    for(double v:o->origin)if(!std::isfinite(v))return fail(-90311,"nonfinite_assembly_origin",error,capacity);
    const double identity6[]={1,0,0,0,1,0};for(size_t i=0;i<6;i++)if(o->csys[i]!=identity6[i])return fail(-90311,"controlled_orientation_not_identity",error,capacity);
    NXOpen::PartLoadStatus *status=nullptr;bool confirmed_owner=false;int32_t result=0;
    try {
        auto owner=part(o->parent);auto assembly=owner->ComponentAssembly();if(!assembly)throw std::runtime_error("parent_assembly_missing");
        NXOpen::Point3d point(o->origin[0],o->origin[1],o->origin[2]);
        NXOpen::Matrix3x3 matrix;memset(&matrix,0,sizeof(matrix));matrix.Xx=1;matrix.Yy=1;matrix.Zz=1;
        out->phase=1;out->call_entered=1;
        auto component=assembly->AddComponent(NXOpen::NXString(o->prototype,NXOpen::NXString::UTF8),NXOpen::NXString(o->refset,NXOpen::NXString::UTF8),NXOpen::NXString(o->name,NXOpen::NXString::UTF8),point,matrix,o->layer,&status);
        out->call_returned=1;confirmed_owner=status!=nullptr;out->status_present=confirmed_owner?1u:0u;out->phase=2;
        if(component)out->component=component->Tag();
        if(!component||!out->component)throw std::runtime_error("null_added_component");
        if(!status)throw std::runtime_error("normal_return_missing_load_status");
        const int count=status->NumberUnloadedParts();out->unloaded_count=count;
        if(count<0||count>static_cast<int>(OE_NX_STATUS_LIMIT))throw std::runtime_error("nxopen_load_status_count_bound");
        for(int i=0;i<count;++i){auto &item=out->items[i];item.code=status->GetStatus(i);copied_text(status->GetPartName(i),item.name,sizeof(item.name));copied_text(status->GetStatusDescription(i),item.description,sizeof(item.description));}
        out->status_copied=1;copied_text(component->ReferenceSet(),out->reference_set,sizeof(out->reference_set));out->complete=1;out->phase=3;
    }catch(const NXOpen::NXException &e){out->exception_code=e.ErrorCode();result=fail(-90312,e.what(),error,capacity);}
     catch(const std::exception &e){result=fail(-90313,e.what(),error,capacity);}
     catch(...){result=fail(-90314,"nxopen_assembly_unknown_exception",error,capacity);}
    if(confirmed_owner){
        // Normal AddComponent return establishes ownership, even if a later
        // observation fails. The only owned vendor object is released once.
        auto release=status;status=nullptr;confirmed_owner=false;
        try{delete release;out->status_released=1;}catch(...){out->complete=0;result=fail(-90315,"nxopen_status_cleanup_exception",error,capacity);}
    }
// If AddComponent threw, the out pointer is never queried/deleted. No fallback.
    return result;
}

static_assert(sizeof(OeNxEntireLoadedAdd)==144 && offsetof(OeNxEntireLoadedAdd,origin)==48
    && offsetof(OeNxEntireLoadedAdd,column_rotation)==72,"NXOpen entire input layout");
static_assert(sizeof(OeNxEntireLoadedObservation)==65920
    && offsetof(OeNxEntireLoadedObservation,vendor_default_refset)==65776,"NXOpen entire result layout");
int32_t oe_nxopen_entire_loaded_add_v1(const OeNxEntireLoadedAdd *o,uint32_t bytes,
    OeNxEntireLoadedObservation *out,uint32_t out_bytes,char *error,uint32_t capacity) noexcept {
    if(!o||!out||!error||bytes!=sizeof(*o)||out_bytes!=sizeof(*out)||capacity==0||capacity>4096
        ||reinterpret_cast<uintptr_t>(o)%alignof(OeNxEntireLoadedAdd)
        ||reinterpret_cast<uintptr_t>(out)%alignof(OeNxEntireLoadedObservation))return -90610;
    const Region regions[]={{o,bytes},{out,out_bytes},{error,capacity}};
    for(size_t i=0;i<3;i++){
        if(!region_valid(regions[i]))return -90610;
        for(size_t j=i+1;j<3;j++)if(overlap(regions[i],regions[j]))return -90610;
    }
    memset(out,0,sizeof(*out));auto &v=out->load;v.version=1;v.structure_bytes=sizeof(v);
    v.unloaded_count=-1;error[0]=0;
    if(o->version!=1||o->structure_bytes!=sizeof(*o)||o->reserved||o->parent==NULL_TAG
        ||o->child==NULL_TAG||o->layer!=-1
        ||!ascii_argument(o->prototype,o->prototype_bytes,MAX_FSPEC_NCHARS+1u)
        ||!ascii_argument(o->name,o->name_bytes,UF_OBJ_NAME_NCHARS+1u))
        return fail(-90611,"invalid_nxopen_entire_input",error,capacity);
    for(double value:o->origin)if(!std::isfinite(value))return fail(-90611,"nonfinite_nxopen_origin",error,capacity);
    for(double value:o->column_rotation)if(!std::isfinite(value))return fail(-90611,"nonfinite_nxopen_rotation",error,capacity);
    NXOpen::PartLoadStatus *status=nullptr;bool confirmed_owner=false;
    NXOpen::Assemblies::AddComponentBuilder *defaults=nullptr;int32_t result=0;
    try {
        auto owner=part(o->parent);auto prototype=part(o->child);
        auto assembly=owner->ComponentAssembly();auto manager=owner->AssemblyManager();
        if(!assembly||!manager)throw std::runtime_error("owned_assembly_manager_missing");
        // Read the actual vendor default, never a guessed localized name.
        defaults=manager->CreateAddComponentBuilder();
        if(!defaults)throw std::runtime_error("default_reference_builder_missing");
        auto initial=defaults->ReferenceSet();copied_text(initial,out->vendor_default_refset,sizeof(out->vendor_default_refset));
        auto release_defaults=defaults;defaults=nullptr;release_defaults->Destroy();out->builder_destroyed=1;
        if(!out->vendor_default_refset[0])throw std::runtime_error("vendor_default_reference_set_empty");
        const auto *r=o->column_rotation;
        NXOpen::Matrix3x3 matrix(r[0],r[1],r[2],r[3],r[4],r[5],r[6],r[7],r[8]);
        NXOpen::Point3d point(o->origin[0],o->origin[1],o->origin[2]);
        v.phase=1;v.call_entered=1;
        auto component=assembly->AddComponent(static_cast<NXOpen::BasePart*>(prototype),initial,
            NXOpen::NXString(o->name,NXOpen::NXString::UTF8),point,matrix,-1,&status);
        v.call_returned=1;confirmed_owner=status!=nullptr;v.status_present=confirmed_owner?1u:0u;v.phase=2;
        if(component)v.component=component->Tag();
        if(!component||!v.component)throw std::runtime_error("normal_return_missing_component");
        if(!status)throw std::runtime_error("normal_return_missing_load_status");
        const int count=status->NumberUnloadedParts();v.unloaded_count=count;
        if(count<0||count>static_cast<int>(OE_NX_STATUS_LIMIT))throw std::runtime_error("nxopen_unloaded_count_bound");
        for(int i=0;i<count;i++){auto &item=v.items[i];item.code=status->GetStatus(i);
            copied_text(status->GetPartName(i),item.name,sizeof(item.name));
            copied_text(status->GetStatusDescription(i),item.description,sizeof(item.description));}
        v.status_copied=1;
        out->direct_owner_matches=component->DirectOwner()==assembly?1u:0u;
        if(count==0 && out->direct_owner_matches==1){
            assembly->SetEntirePartRefset(component);out->entire_set_returned=1;
            copied_text(component->ReferenceSet(),v.reference_set,sizeof(v.reference_set));
            copied_text(component->EntirePartRefsetName(),out->entire_refset,sizeof(out->entire_refset));
        }
        v.complete=1;v.phase=3;
    }catch(const NXOpen::NXException &e){v.exception_code=e.ErrorCode();result=fail(-90612,e.what(),error,capacity);}
     catch(const std::exception &e){result=fail(-90613,e.what(),error,capacity);}
     catch(...){result=fail(-90614,"nxopen_entire_unknown_exception",error,capacity);}
    if(defaults){auto release=defaults;defaults=nullptr;try{release->Destroy();out->builder_destroyed=1;}
        catch(...){v.complete=0;result=fail(-90615,"default_builder_cleanup_exception",error,capacity);}}
    // The SDK declares exception outputs unusable. Only a normal return
    // confirms this transient object's ownership, including later copy errors.
    if(confirmed_owner){auto release=status;status=nullptr;confirmed_owner=false;
        try{delete release;v.status_released=1;}
        catch(...){v.complete=0;result=fail(-90616,"nxopen_status_cleanup_exception",error,capacity);}}
    return result;
}

static_assert(std::is_same_v<decltype(&UF_OBJ_ask_status),int(*)(tag_t)>,"object status ABI differs");
static_assert(std::is_same_v<decltype(&UF_SO_ask_visibility_option),int(*)(tag_t,UF_SO_visibility_option_p_t)>,"smart visibility query ABI differs");
static_assert(std::is_same_v<decltype(&UF_OBJ_ask_display_properties),int(*)(tag_t,UF_OBJ_disp_props_p_t)>,"object display properties ABI differs");
int32_t oe_nx_make_layout_anchor_visible_v1(uint32_t object) noexcept {
    if(!object)return -90004;
    return UF_SO_set_visibility_option(object,UF_SO_visible);
}
int32_t oe_nx_ask_layout_visibility_v1(uint32_t object,int32_t codes[2],int32_t values[8]) noexcept {
    if(!object || !codes || !values)return -90004;
    std::fill(codes,codes+2,-1);std::fill(values,values+8,-1);
    values[1]=UF_SO_invisible;values[2]=UF_SO_visible;values[3]=UF_SO_visible_if_parent_invisible;
    values[6]=UF_OBJ_BLANKED;values[7]=UF_OBJ_NOT_BLANKED;
    UF_SO_visibility_option_t visibility=UF_SO_invisible;
    codes[0]=UF_SO_ask_visibility_option(object,&visibility);
    if(codes[0]==0)values[0]=visibility;
    UF_OBJ_disp_props_t properties={};
    codes[1]=UF_OBJ_ask_display_properties(object,&properties);
    if(codes[1]==0){values[4]=properties.layer;values[5]=properties.blank_status;}
    return 0;
}
int32_t oe_nx_ask_object_state_v1(uint32_t object,int32_t *status,int32_t states[4]) noexcept {
    if(!object || !status || !states)return -90004;
    states[0]=UF_OBJ_DELETED;states[1]=UF_OBJ_TEMPORARY;
    states[2]=UF_OBJ_CONDEMNED;states[3]=UF_OBJ_ALIVE;
    *status=UF_OBJ_ask_status(object);
    return 0;
}

int32_t oe_nx_close_control_part_v1(uint32_t part) noexcept {
    return UF_PART_close(static_cast<tag_t>(part), 0, 1);
}

int32_t oe_nx_import_drafting_view_without_anchor_v1(uint32_t drawing,uint32_t model_view,const char *model_name,
    double scale,const double reference[2],uint32_t *view,int32_t *query_code,char *error,uint32_t capacity) noexcept {
    if(!drawing || !model_view || !model_name || !reference || !view || !query_code)return -90004;
    *view=0;*query_code=-1;
    return guarded([&]{
        const size_t length=strlen(model_name);
        if(!length || length>=UF_DRAW_MAX_LABEL_LEN)throw std::runtime_error("native_drawing_model_name_bound");
        UF_DRAW_view_info_t info={};UF_DRAW_initialize_view_info(&info);
        info.view_scale=scale;info.anchor_point=NULL_TAG;info.use_ref_pt=false;info.inherit_boundary=false;
        info.transfer_annotation=false;info.inherit_pmi=false;
        memcpy(info.model_name,model_name,length+1);
        double point[2]={reference[0],reference[1]};
        *query_code=UF_DRAW_import_view(drawing,model_view,point,&info,view);
        if(*query_code!=0)throw std::runtime_error("native_drawing_optional_anchor_import_failed");
    },error,capacity);
}

int32_t oe_nx_sheet_parallel_reference_v1(uint32_t edge1,uint32_t edge2,uint32_t view,
    const double drawing_origin[3],uint32_t *dimension,int32_t *native_code,char *error,uint32_t error_capacity) noexcept {
    if(!edge1 || !edge2 || edge1==edge2 || !view || !drawing_origin || !dimension || !native_code)return -90004;
    *dimension=0;*native_code=-1;
    return guarded([&]{
        UF_DRF_object_t first={},second={};
        first.object_tag=edge1;second.object_tag=edge2;
        first.object_view_tag=view;second.object_view_tag=view;
        first.object_assoc_type=UF_DRF_arc_center;second.object_assoc_type=UF_DRF_arc_center;
        UF_DRF_text_t text={};double origin[3]={drawing_origin[0],drawing_origin[1],drawing_origin[2]};
        *native_code=UF_DRF_create_parallel_dim(&first,&second,&text,origin,dimension);
        if(*native_code!=0)throw std::runtime_error("sheet_reference_creation_failed");
        if(!*dimension)throw std::runtime_error("sheet_reference_dimension_null");
    },error,error_capacity);
}

static_assert(NXOpen::Assemblies::PositionOverrideTypeNone==0
    && NXOpen::Assemblies::PositionOverrideTypeUnloaded==1
    && NXOpen::Assemblies::PositionOverrideTypeExplicit==2
    && NXOpen::Assemblies::PositionOverrideTypeMatingImplicit==3
    && NXOpen::Assemblies::PositionOverrideTypeConstraintImplicit==4,
    "position override enum differs from reviewed SDK");
int32_t oe_nx_position_override_v1(uint32_t component,int32_t *raw_type,
    uint32_t *parent,char *error,uint32_t capacity) noexcept {
    if(!component || !raw_type || !parent)return -90004;
    *raw_type=-1;*parent=0;
    return guarded([&]{
        if(!NXOpen::Session::GetSession())throw std::runtime_error("vendor_cpp_session_missing");
        auto c=dynamic_cast<NXOpen::Assemblies::Component*>(NXOpen::NXObjectManager::Get(component));
        if(!c)throw std::runtime_error("position_tag_not_component");
        const auto type=c->GetPositionOverrideType();
        auto p=c->GetPositionOverrideParent();
        *raw_type=static_cast<int32_t>(type);*parent=p?p->Tag():0;
    },error,capacity);
}

static_assert(std::is_same_v<decltype(&UF_ASSEM_reposition_part_occurrence),int(*)(tag_t,double(*)[4],UF_ASSEM_level_option_t)>);
static_assert(std::is_same_v<decltype(&UF_MODL_check_interference),int(*)(tag_t,int,tag_t*,int*)>);
static_assert(std::is_same_v<decltype(&UF_MODL_update),int(*)()>);
static_assert(UF_ASSEM_establish_override!=UF_ASSEM_use_existing_level && UF_ASSEM_establish_override!=UF_ASSEM_use_strict_level);
int32_t oe_nx_reposition_occurrence_v1(uint32_t component,const double delta[16],int32_t *native_code) noexcept {
    if(!component || !delta || !native_code)return -90004;
    *native_code=-1;double transform[4][4];
    for(size_t i=0;i<16;++i){if(!std::isfinite(delta[i]))return -90004;transform[i/4][i%4]=delta[i];}
    *native_code=UF_ASSEM_reposition_part_occurrence(component,transform,UF_ASSEM_establish_override);
    return 0;
}

using OeNxCopySignature1=std::vector<NXOpen::Assemblies::Component*> (NXOpen::Assemblies::ComponentAssembly::*)(const std::vector<NXOpen::Assemblies::Component*>&);
static_assert(std::is_same_v<decltype(&NXOpen::Assemblies::ComponentAssembly::CopyComponents),OeNxCopySignature1>,
    "CopyComponents signature differs from captured NX2412 SDK");
int32_t oe_nx_component_copy_v1(const OeNxComponentCopyRequest1 *r,uint32_t rbytes,
    OeNxComponentCopyObservation1 *out,uint32_t obytes,char *error,uint32_t capacity) noexcept {
    if(!r||rbytes!=sizeof(*r)||reinterpret_cast<uintptr_t>(r)%alignof(OeNxComponentCopyRequest1)
       ||!out||obytes!=sizeof(*out)||reinterpret_cast<uintptr_t>(out)%alignof(OeNxComponentCopyObservation1)
       ||!error||capacity==0||capacity>4096||!region_valid({r,rbytes})||!region_valid({out,obytes})
       ||!region_valid({error,capacity})||overlap({r,rbytes},{out,obytes})||overlap({r,rbytes},{error,capacity})
       ||overlap({out,obytes},{error,capacity}))return -90400;
    memset(out,0,sizeof(*out));out->version=1;out->structure_bytes=sizeof(*out);error[0]=0;
    if(r->version!=1||r->structure_bytes!=sizeof(*r)||r->reserved||!r->parent||!r->source_component
       ||!r->name||r->name_bytes<2||r->name_bytes>65||!region_valid({r->name,r->name_bytes})
       ||overlap({r->name,r->name_bytes},{out,obytes})||overlap({r->name,r->name_bytes},{error,capacity})
       ||memchr(r->name,0,r->name_bytes)!=r->name+r->name_bytes-1)
        return fail(-90401,"component_copy_request_invalid",error,capacity);
    for(uint32_t k=0;k+1<r->name_bytes;++k){const auto c=static_cast<unsigned char>(r->name[k]);
        if(!((c>='A'&&c<='Z')||(c>='a'&&c<='z')||(c>='0'&&c<='9')||c=='_'))
            return fail(-90402,"component_copy_instance_name_invalid",error,capacity);}
    const auto code=guarded([&]{
        auto owner=part(r->parent);auto assembly=owner->ComponentAssembly();
        auto source=dynamic_cast<NXOpen::Assemblies::Component*>(NXOpen::NXObjectManager::Get(r->source_component));
        if(!assembly||!source||!UF_ASSEM_is_occurrence(r->source_component)
           ||UF_PART_ask_display_part()!=r->parent||UF_ASSEM_ask_work_part()!=r->parent)
            throw std::runtime_error("component_copy_owned_context_differs");
        out->source_component=r->source_component;
        out->source_instance=UF_ASSEM_ask_inst_of_part_occ(r->source_component);
        out->source_parent=UF_ASSEM_ask_parent_of_instance(out->source_instance);
        out->source_prototype=UF_ASSEM_ask_child_of_instance(out->source_instance);
        if(!out->source_instance||out->source_parent!=r->parent||!out->source_prototype
           ||UF_ASSEM_ask_prototype_of_occ(r->source_component)!=out->source_prototype)
            throw std::runtime_error("component_copy_current_source_differs");
        // Normal SDK return may be partial. No output component is read after an
        // exception, no missing result is filled, and there is no Add fallback.
        out->call_entered=1;
        const auto copied=assembly->CopyComponents({source});
        out->call_returned=1;out->returned_count=copied.size();
        out->captured_count=static_cast<uint32_t>(std::min<size_t>(copied.size(),64));
        for(uint32_t k=0;k<out->captured_count;++k)out->returned_components[k]=copied[k]?copied[k]->Tag():0;
        if(copied.size()!=1||!copied[0]||!out->returned_components[0]
           ||out->returned_components[0]==r->source_component||!UF_ASSEM_is_occurrence(out->returned_components[0]))
            throw std::runtime_error("component_copy_partial_or_reused_result");
        out->new_instance=UF_ASSEM_ask_inst_of_part_occ(out->returned_components[0]);
        out->new_parent=UF_ASSEM_ask_parent_of_instance(out->new_instance);
        out->new_prototype=UF_ASSEM_ask_child_of_instance(out->new_instance);
        if(!out->new_instance||out->new_instance==out->source_instance
           ||out->new_parent!=out->source_parent||out->new_prototype!=out->source_prototype)
            throw std::runtime_error("component_copy_new_lineage_differs");
        out->rename_attempted=1;out->rename_code=UF_ASSEM_rename_instance(out->new_instance,r->name);
        if(out->rename_code)throw std::runtime_error("component_copy_instance_rename_failed");
        out->complete=1;
    },error,capacity);
    out->exception_code=code;
    return code;
}

// Native ABI only: source policy, geometry invariants and acceptance are Rust-owned.
int32_t oe_nx_drawing_pdf_v1(uint32_t part_tag,uint32_t sheet_tag,const char *output,int32_t *o,uint32_t count,char *error,uint32_t capacity){
    if(!o || count!=10 || !output || !*output)return fail(-90700,"pdf_invalid_abi",error,capacity);
    std::fill(o,o+10,0);o[0]=1;
    NXOpen::PrintPDFBuilder *builder=nullptr;
    int32_t result=guarded([&]{
        auto p=part(part_tag);
        o[1]=UF_PART_ask_display_part()==part_tag;
        tag_t owner=0;int rc=UF_OBJ_ask_owning_part(sheet_tag,&owner);o[2]=rc==0 && owner==part_tag;
        UF_DRAW_info_t info{};int sheet_code=UF_DRAW_ask_drawing_info(sheet_tag,&info);
        auto sheet=dynamic_cast<NXOpen::NXObject*>(NXOpen::NXObjectManager::Get(sheet_tag));
        if(!o[1] || !o[2] || !sheet || sheet_code!=0)throw std::runtime_error("pdf_sheet_or_display_owner_mismatch");
        builder=p->PlotManager()->CreatePrintPdfbuilder();
        using B=NXOpen::PrintPDFBuilder;
        builder->SetAction(B::ActionOptionNative);builder->SetFilename(output);
        builder->SetColors(B::ColorBlackOnWhite);builder->SetWidths(B::WidthStandardWidths);
        builder->SetSize(B::SizeOptionFullScale);builder->SetScale(1.0);builder->SetUnits(B::UnitsOptionMetric);
        builder->SetOutputText(B::OutputTextOptionText);builder->SetAppend(false);
        builder->SetAddWatermark(false);builder->SetRasterImages(false);
        auto source=builder->SourceBuilder();source->SetSheets(std::vector<NXOpen::NXObject*>{sheet});source->SetSelectedSheets(1);
        o[3]=source->SelectedSheets();auto selected=source->GetSheets();o[4]=selected.size()==1 && selected[0]->Tag()==sheet_tag;
        o[5]=builder->Action()==B::ActionOptionNative && builder->Colors()==B::ColorBlackOnWhite && builder->Widths()==B::WidthStandardWidths
            && builder->Size()==B::SizeOptionFullScale && builder->Scale()==1.0 && builder->Units()==B::UnitsOptionMetric
            && builder->OutputText()==B::OutputTextOptionText && !builder->Append() && !builder->AddWatermark() && !builder->RasterImages();
        if(o[3]!=1 || !o[4] || !o[5])throw std::runtime_error("pdf_builder_selection_or_settings_mismatch");
        o[6]=builder->Validate();if(!o[6])throw std::runtime_error("pdf_builder_validation_failed");
        o[7]=1;builder->Commit();o[7]=2;
    },error,capacity);
    if(result)o[9]=result;
    if(builder){int32_t cleanup=guarded([&]{builder->Destroy();o[8]=1;},result?nullptr:error,result?0:capacity);if(!result && cleanup)result=cleanup;}
    return result;
}

int32_t oe_nx_dimension_label_v1(uint32_t p,uint32_t tag,const char*label,int32_t*o,uint32_t count,char*below,uint32_t cap,char*error,uint32_t error_cap){
    if(!o || count!=8 || !below || cap<1)return fail(-90710,"label_invalid_abi",error,error_cap);
    std::fill(o,o+8,-1);o[0]=1;below[0]=0;
    return guarded([&]{
        tag_t owner=0;int type=0,subtype=0;o[1]=UF_OBJ_ask_owning_part(tag,&owner)==0 && owner==p;
        o[2]=UF_OBJ_ask_type_and_subtype(tag,&type,&subtype)==0 && type==26 && subtype==3;
        if(!o[1] || !o[2])throw std::runtime_error("label_dimension_owner_or_type");
        auto d=dynamic_cast<NXOpen::Annotations::Dimension*>(NXOpen::NXObjectManager::Get(tag));if(!d)throw std::runtime_error("label_dimension_missing");
        std::unique_ptr<NXOpen::Annotations::AppendedText> text(d->GetAppendedText());if(!text)throw std::runtime_error("label_appended_text_missing");
        o[7]=0;
        if(label){std::vector<NXOpen::NXString> lines;if(*label)lines.emplace_back(label);text->SetBelowText(lines);d->SetAppendedText(text.get());o[7]=1;}
        text.reset(d->GetAppendedText());
        auto b=text->GetBelowText();o[3]=static_cast<int32_t>(text->GetBeforeText().size());o[4]=static_cast<int32_t>(text->GetAfterText().size());o[5]=static_cast<int32_t>(text->GetAboveText().size());o[6]=static_cast<int32_t>(b.size());
        if(b.size()==1){auto value=b[0].GetUTF8Text();size_t size=strlen(value);if(size>=cap)throw std::runtime_error("label_readback_budget");memcpy(below,value,size+1);}
    },error,error_cap);
}
int32_t oe_nx_drawing_wcs_v1(uint32_t p,int32_t requested,int32_t*o,uint32_t count,char*error,uint32_t cap){
    if(!o || count!=4 || requested< -1 || requested>1)return fail(-90711,"wcs_invalid_abi",error,cap);
    std::fill(o,o+4,-1);o[0]=1;
    return guarded([&]{o[1]=UF_PART_ask_display_part()==p;if(!o[1])throw std::runtime_error("wcs_display_part_mismatch");auto w=part(p)->WCS();if(!w)throw std::runtime_error("wcs_missing");o[3]=0;if(requested>=0){w->SetVisibility(requested==1);o[3]=1;}o[2]=w->Visibility()?1:0;},error,cap);
}


int32_t oe_nx_dimension_label_v2(uint32_t p,uint32_t tag,const char*label,int32_t*o,uint32_t count,char*below,uint32_t cap,char*error,uint32_t error_cap){
    if(!o || count!=8 || !below || cap<1)return fail(-90720,"label_lines_invalid_abi",error,error_cap);
    std::fill(o,o+8,-1);o[0]=2;below[0]=0;
    return guarded([&]{
        tag_t owner=0;int type=0,subtype=0;o[1]=UF_OBJ_ask_owning_part(tag,&owner)==0 && owner==p;
        o[2]=UF_OBJ_ask_type_and_subtype(tag,&type,&subtype)==0 && type==26 && subtype==3;
        if(!o[1] || !o[2])throw std::runtime_error("label_lines_dimension_owner_or_type");
        auto d=dynamic_cast<NXOpen::Annotations::Dimension*>(NXOpen::NXObjectManager::Get(tag));if(!d)throw std::runtime_error("label_lines_dimension_missing");
        std::unique_ptr<NXOpen::Annotations::AppendedText> text(d->GetAppendedText());if(!text)throw std::runtime_error("label_lines_text_missing");o[7]=0;
        if(label){
            std::vector<NXOpen::NXString> lines;
            if(*label){const char*split=strchr(label,'\n');if(!split||strchr(split+1,'\n')||split==label||!split[1])throw std::runtime_error("label_lines_count");
                std::string first(label,static_cast<size_t>(split-label));lines.emplace_back(first.c_str());lines.emplace_back(split+1);}
            text->SetBelowText(lines);d->SetAppendedText(text.get());o[7]=1;
        }
        text.reset(d->GetAppendedText());auto b=text->GetBelowText();o[3]=static_cast<int32_t>(text->GetBeforeText().size());o[4]=static_cast<int32_t>(text->GetAfterText().size());o[5]=static_cast<int32_t>(text->GetAboveText().size());o[6]=static_cast<int32_t>(b.size());
        std::string joined;for(size_t n=0;n<b.size();++n){if(n)joined+='\n';joined+=b[n].GetUTF8Text();if(joined.size()>=cap)throw std::runtime_error("label_lines_readback_budget");}
        memcpy(below,joined.c_str(),joined.size()+1);
    },error,error_cap);
}

// Single, synchronous, nonnested vendor edit transaction. Rust judges all evidence.
int32_t oe_nx_dimension_label_update_v1(uint32_t p,uint32_t tag,const char*label,int32_t*o,uint32_t count,char*below,uint32_t cap,char*error,uint32_t error_cap,int32_t*u,uint32_t un,int32_t*codes,uint32_t cn){
 if(!p||!tag||!label||!o||count!=8||!below||!cap||!u||un!=30||!codes||cn!=64)return fail(-90740,"local_update_abi",error,error_cap);
 std::fill(u,u+30,-1);std::fill(codes,codes+64,0);std::fill(o,o+8,-1);below[0]=0;u[0]=1;
 for(int n:{5,6,8,9,10,12,13,16,17,18,19,20,21,28})u[n]=0;
 return guarded([&]{
  auto s=NXOpen::Session::GetSession();auto m=s->UpdateManager();
  u[1]=UF_PART_ask_display_part()==p;
  std::unique_ptr<NXOpen::ErrorList> initial(m->ErrorList());u[2]=initial->Length();
  u[3]=static_cast<int32_t>(m->GetObjectsOnDeleteList().size());u[4]=m->GetUpdateLock()?1:0;
  auto capture_errors=[&]{std::unique_ptr<NXOpen::ErrorList> list(m->ErrorList());u[22]=list->Length();u[29]=std::min(u[22],64);for(int n=0;n<u[29];++n){std::unique_ptr<NXOpen::ErrorInfo> info(list->GetErrorInfo(n));codes[n]=info->ErrorCode();}};
  capture_errors();
  if(!u[1]||u[2]!=0||u[3]!=0||u[4]!=0)throw std::runtime_error("local_update_existing_state_rejected");
  auto prior=s->GetAllUndoMarks(NXOpen::Session::MarkVisibilityAnyVisibility);
  NXOpen::Session::UndoMarkId mark=NXOpen::Session::UndoMarkIdDummyMember;
  std::exception_ptr failed;
  try{
   u[5]=1;mark=s->SetUndoMark(NXOpen::Session::MarkVisibilityInvisible,"oev2 label edit update v1");u[6]=1;u[7]=static_cast<int32_t>(mark);
   u[8]=1;m->StartLocalUpdate();u[9]=1;
   u[10]=1;u[11]=oe_nx_dimension_label_v2(p,tag,label,o,count,below,cap,error,error_cap);
   if(u[11]!=0)throw std::runtime_error("local_update_label_failed");
   u[12]=1;u[14]=m->DoUpdate(mark);u[13]=1;
   std::unique_ptr<NXOpen::ErrorList> list(m->ErrorList());u[15]=list->Length();
   if(u[14]!=0||u[15]!=0)throw std::runtime_error("local_update_errors");
  }catch(...){failed=std::current_exception();}
  auto cleanup=[&](auto fn){try{fn();}catch(...){++u[28];if(!failed)failed=std::current_exception();}};
  // End only a local scope whose start actually returned; never end another caller's scope.
  if(u[9]==1){u[16]=1;cleanup([&]{m->EndLocalUpdate();u[17]=1;});}
  if(failed&&u[6]==1){u[18]=1;cleanup([&]{s->UndoToMark(mark,static_cast<const char*>(nullptr));u[19]=1;});}
  if(u[6]==1){u[20]=1;cleanup([&]{s->DeleteUndoMark(mark,static_cast<const char*>(nullptr));u[21]=1;});}
  cleanup([&]{capture_errors();u[23]=static_cast<int32_t>(m->GetObjectsOnDeleteList().size());u[24]=m->GetUpdateLock()?1:0;});
  cleanup([&]{auto now=s->GetAllUndoMarks(NXOpen::Session::MarkVisibilityAnyVisibility);u[25]=now.size()==prior.size();u[26]=1;for(const auto& n:now){if(u[6]==1&&n.Id==mark)u[26]=0;auto found=std::find_if(prior.begin(),prior.end(),[&](const auto& old){return old.Id==n.Id&&old.Visibility==n.Visibility;});if(found==prior.end())u[25]=0;}});
  cleanup([&]{int32_t wrote=o[7];u[27]=oe_nx_dimension_label_v2(p,tag,nullptr,o,count,below,cap,error,error_cap);o[7]=wrote;if(u[27]!=0)throw std::runtime_error("local_update_readback_failed");});
  if(failed)std::rethrow_exception(failed);
 },error,error_cap);
}

int32_t oe_nx_drawing_csys_v1(uint32_t *wcs,int32_t*kind) noexcept{
    if(!wcs||!kind)return -90004;*wcs=0;*kind=UF_coordinate_system_type;return UF_CSYS_ask_wcs(wcs);
}
int32_t oe_nx_dimension_layout_v1(uint32_t p,uint32_t tag,const double*origin,int32_t centered,int32_t*o,uint32_t count,double*actual,char*error,uint32_t cap){
    if(!p||!tag||!o||count!=14||!actual||centered< -1||centered>1)return fail(-90730,"annotation_layout_invalid_abi",error,cap);
    std::fill(o,o+14,-1);std::fill(actual,actual+3,std::numeric_limits<double>::quiet_NaN());o[0]=1;
    o[8]=0;o[9]=0;o[10]=0;o[11]=0;o[12]=0;
    return guarded([&]{
        o[1]=UF_OBJ_ask_status(tag);o[2]=UF_OBJ_ALIVE;
        if(o[1]!=o[2])throw std::runtime_error("annotation_layout_not_alive");
        tag_t owner=0;int type=0,subtype=0;
        o[3]=UF_OBJ_ask_owning_part(tag,&owner)==0 && owner==p;
        o[4]=UF_OBJ_ask_type_and_subtype(tag,&type,&subtype)==0 && type==26 && subtype==3;
        if(!o[3]||!o[4])throw std::runtime_error("annotation_layout_owner_or_type");
        auto d=dynamic_cast<NXOpen::Annotations::Dimension*>(NXOpen::NXObjectManager::Get(tag));
        if(!d)throw std::runtime_error("annotation_layout_dimension_missing");
        o[13]=d->HasAssociativeOrigin()?1:0;
        if((origin||centered>=0)&&o[13])throw std::runtime_error("annotation_layout_associative_origin_unsupported");
        if(origin){for(size_t n=0;n<3;++n)if(!std::isfinite(origin[n]))throw std::runtime_error("annotation_layout_nonfinite_origin");}
        // The Rust owner selects changed state. Preserve this order because
        // restoring centering can itself relocate the text origin.
        if(centered>=0){o[8]=1;d->SetOriginCentered(centered==1);o[9]=1;}
        if(origin){o[10]=1;NXOpen::Point3d point;point.X=origin[0];point.Y=origin[1];point.Z=origin[2];d->SetAnnotationOrigin(point);o[11]=1;}
        auto point=d->AnnotationOrigin();actual[0]=point.X;actual[1]=point.Y;actual[2]=point.Z;
        o[5]=d->IsOriginCentered()?1:0;o[6]=d->HasAssociativeOrigin()?1:0;o[12]=1;
    },error,cap);
}
int32_t oe_nx_drawing_object_v1(uint32_t p,uint32_t tag,int32_t requested,int32_t*o,uint32_t count,uint32_t*tags,double*data,char*error,uint32_t cap){
    if(!p||!tag||!o||count!=20||!tags||!data||requested< -1||requested>1)return fail(-90721,"display_object_invalid_abi",error,cap);
    std::fill(o,o+20,-1);std::fill(tags,tags+3,0u);std::fill(data,data+12,std::numeric_limits<double>::quiet_NaN());
    o[0]=1;o[1]=UF_OBJ_ask_status(tag);o[2]=UF_OBJ_ALIVE;o[10]=UF_OBJ_BLANKED;o[11]=UF_OBJ_NOT_BLANKED;
    return guarded([&]{
        if(o[1]!=o[2]){if(requested>=0)throw std::runtime_error("display_object_not_alive");return;}
        o[3]=UF_OBJ_ask_type_and_subtype(tag,&o[4],&o[5]);o[6]=UF_OBJ_ask_owning_part(tag,&tags[0]);o[12]=UF_CSYS_ask_wcs(&tags[1]);o[16]=(o[3]==0 && o[4]==UF_coordinate_system_type)?1:0;
        UF_OBJ_disp_props_t props={};o[7]=UF_OBJ_ask_display_properties(tag,&props);
        if(requested>=0){
            if(o[16]!=1||o[6]!=0||tags[0]!=p||o[12]!=0||!tags[1]||tags[1]==tag||o[7]!=0)throw std::runtime_error("display_coordinate_write_identity");
            o[15]=UF_OBJ_set_blank_status(tag,requested?UF_OBJ_BLANKED:UF_OBJ_NOT_BLANKED);
            if(o[15]!=0)throw std::runtime_error("display_coordinate_write_failed");
            o[7]=UF_OBJ_ask_display_properties(tag,&props);
        }
        if(o[7]==0){o[8]=props.layer;o[9]=props.blank_status;o[17]=props.color;o[18]=props.font;o[19]=props.line_width;}
        if(o[16]==1){o[13]=UF_CSYS_ask_csys_info(tag,&tags[2],data);if(o[13]==0&&tags[2])o[14]=UF_CSYS_ask_matrix_values(tags[2],data+3);}
    },error,cap);
}

int32_t oe_nx_restore_drawing_wcs_v1(uint32_t p,uint32_t original,int32_t*o,uint32_t count,uint32_t*tags,char*error,uint32_t cap){
 if(!p||!original||!o||count!=11||!tags)return fail(-90730,"wcs_restore_invalid_abi",error,cap);
 std::fill(o,o+11,-1);std::fill(tags,tags+3,0u);o[0]=1;o[1]=UF_OBJ_ask_status(original);o[2]=UF_OBJ_ALIVE;o[6]=UF_coordinate_system_type;o[10]=0;
 return guarded([&]{
  if(o[1]!=o[2])throw std::runtime_error("wcs_restore_original_not_alive");
  int subtype=-1;o[3]=UF_OBJ_ask_owning_part(original,&tags[0]);o[4]=UF_OBJ_ask_type_and_subtype(original,&o[5],&subtype);
  if(o[3]!=0||tags[0]!=p||o[4]!=0||o[5]!=o[6])throw std::runtime_error("wcs_restore_original_identity");
  o[7]=UF_CSYS_ask_wcs(&tags[1]);if(o[7]!=0||!tags[1])throw std::runtime_error("wcs_restore_current_unknown");
  o[8]=0;if(tags[1]!=original){o[10]=1;o[8]=UF_CSYS_set_wcs(original);if(o[8]!=0)throw std::runtime_error("wcs_restore_set_failed");}
  o[9]=UF_CSYS_ask_wcs(&tags[2]);if(o[9]!=0||tags[2]!=original)throw std::runtime_error("wcs_restore_readback_differs");
 },error,cap);
}

static_assert(UF_DRF_NO_POINT_TYPE == 0);
static_assert(UF_dim_diameter_subtype == 10 && UF_DRF_ARC_CENTER_POINT_TYPE == 2 && UF_DRF_NO_LINE_TYPE == 0);
static_assert(std::is_same_v<decltype(&UF_DRF_create_diameter_dim),int(*)(UF_DRF_object_p_t,UF_DRF_text_t*,double*,tag_t*)>);
int32_t oe_nx_sheet_diameter_v1(uint32_t edge,uint32_t view,const double drawing_origin[3],
    uint32_t *dimension,int32_t *native_code,char *error,uint32_t error_capacity) noexcept {
    if(!edge || !view || !drawing_origin || !dimension || !native_code)return -90004;
    *dimension=0;*native_code=-1;
    return guarded([&]{
        UF_DRF_object_t object={};object.object_tag=edge;object.object_view_tag=view;
        object.object_assoc_type=UF_DRF_arc_center;
        UF_DRF_text_t text={};double origin[3]={drawing_origin[0],drawing_origin[1],drawing_origin[2]};
        *native_code=UF_DRF_create_diameter_dim(&object,&text,origin,dimension);
        if(*native_code!=0)throw std::runtime_error("sheet_diameter_creation_failed");
        if(!*dimension)throw std::runtime_error("sheet_diameter_null");
    },error,error_capacity);
}
