#pragma once
#include <stdint.h>
#define OE_NX_ABI_VERSION 2u
#ifdef _WIN32
#define OE_NX_EXPORT extern "C" __declspec(dllexport)
#else
#define OE_NX_EXPORT extern "C"
#endif
// No vendor C++ objects or exceptions cross this ABI.
// Only copied POD observations cross the new NXOpen assembly boundary.
struct OeNxAbi {
    uint32_t version, structure_bytes, tag_bytes, logical_bytes, load_status_bytes;
    uint32_t load_status_failed, load_status_abort, load_status_count, load_status_names, load_status_codes;
    int32_t metric, english, solid_type, solid_body, sheet_body, planar_face, null_sign;
    int32_t step_ap214, step_existing_part, step_entire_part, step_new_part;
    int32_t load_from_directory, load_all;
    uint32_t max_path_bytes, max_refset_bytes, max_instance_bytes;
    uint32_t jt_observation_version, jt_observation_bytes, jt_observation_offsets[12];
    int32_t faceted_type, jt_body_subtype;
};
struct OeNxStepExport {
    uint32_t structure_bytes;
    int32_t application_protocol, export_from, selection_scope;
    uint32_t solids, file_save, process_hold;
    const char *input, *output, *layers;
};
struct OeNxStepImport {
    uint32_t structure_bytes;
    int32_t import_to;
    uint32_t solids, file_open, process_hold;
    const char *input, *output;
};
OE_NX_EXPORT int32_t oe_nx_abi(OeNxAbi *out, uint32_t bytes) noexcept;
OE_NX_EXPORT int32_t oe_nx_expression_create(uint32_t part, const char *formula, const char *unit,
    uint32_t *expression, char *error, uint32_t error_capacity) noexcept;
OE_NX_EXPORT int32_t oe_nx_expression_edit_update(uint32_t part, uint32_t expression, const char *formula,
    int32_t *update_errors, char *error, uint32_t error_capacity) noexcept;
OE_NX_EXPORT int32_t oe_nx_load_options(int32_t method, int32_t components, uint32_t partial,
    uint32_t generate_missing, char *error, uint32_t error_capacity) noexcept;
OE_NX_EXPORT int32_t oe_nx_step_export(const OeNxStepExport *options, char *error, uint32_t error_capacity) noexcept;
OE_NX_EXPORT int32_t oe_nx_step_import(const OeNxStepImport *options, char *error, uint32_t error_capacity) noexcept;

#define OE_NX_CONTROLLED_REFSET "OE_CONTROLLED_BODY"
// Independent POD extension. Source tags are validated only in the current
// session; one CopyComponents return may be partial and never implies adoption.
struct OeNxComponentCopyRequest1 {
    uint32_t version,structure_bytes,parent,source_component,name_bytes,reserved;
    const char *name;
};
struct OeNxComponentCopyObservation1 {
    uint32_t version,structure_bytes,call_entered,call_returned;
    uint32_t source_component,source_instance,source_parent,source_prototype;
    uint64_t returned_count;
    uint32_t captured_count,returned_components[64];
    uint32_t new_instance,new_parent,new_prototype,rename_attempted;
    int32_t rename_code;
    uint32_t complete;
    int32_t exception_code;
};
OE_NX_EXPORT int32_t oe_nx_component_copy_v1(const OeNxComponentCopyRequest1 *request,
    uint32_t request_bytes,OeNxComponentCopyObservation1 *out,uint32_t out_bytes,
    char *error,uint32_t error_capacity) noexcept;
#define OE_NX_EXTENSION_VERSION 1u
struct OeNxExtensionAbi { uint32_t version, structure_bytes; int32_t negative_sign, dimension_type, drafting_entity_type; };
struct OeNxTextBox { double origin[3], length, height, angle_radians, distance; };
OE_NX_EXPORT int32_t oe_nx_extension_abi(OeNxExtensionAbi *out,uint32_t bytes) noexcept;
OE_NX_EXPORT int32_t oe_nx_annotation_boxes(uint32_t annotation,OeNxTextBox *out,uint32_t capacity,
    uint32_t *count,char *error,uint32_t error_capacity) noexcept;
// Separate extension preserves the established ABI2 and extension1 layouts.
struct OeNxAssociation { uint32_t object_1,object_2,view; int32_t point_type,line_type; double position[3]; };
struct OeNxAssociationAbi { uint32_t version,structure_bytes,entry_bytes,entry_offsets[6]; int32_t face_subtype,edge_subtype; };
OE_NX_EXPORT int32_t oe_nx_association_abi(OeNxAssociationAbi *out,uint32_t bytes) noexcept;
OE_NX_EXPORT int32_t oe_nx_annotation_associations(uint32_t annotation,OeNxAssociation *out,uint32_t capacity,
    uint32_t *count,int32_t *query_code,char *error,uint32_t error_capacity) noexcept;
// Separate POD ABI: the reviewed base/association layouts stay unchanged.
struct OeNxModelEdge {
    int32_t initialize_code,is_arc_code,arc_code,free_code;
    uint32_t is_arc,is_periodic;
    double limits[2],radius,center[3],x_axis[3],y_axis[3];
};
struct OeNxModelAbi { uint32_t version,structure_bytes,edge_bytes,edge_offsets[11]; };
OE_NX_EXPORT int32_t oe_nx_model_abi(OeNxModelAbi *out,uint32_t bytes) noexcept;
OE_NX_EXPORT int32_t oe_nx_model_edge(uint32_t edge,OeNxModelEdge *out,uint32_t bytes,
    char *error,uint32_t error_capacity) noexcept;
OE_NX_EXPORT int32_t oe_nx_dimension_value(uint32_t dimension,double *out,
    char *error,uint32_t error_capacity) noexcept;
// codes: ask_wcs, create_matrix, create_csys, set_wcs, create_dimension, restore_wcs.
OE_NX_EXPORT int32_t oe_nx_parallel_reference(uint32_t edge1,uint32_t edge2,
    const double plane_origin[3],const double plane_matrix[9],const double text_origin_wcs[3],
    uint32_t *dimension,int32_t codes[6],char *error,uint32_t error_capacity) noexcept;
// Versioned scalar drafting extension: SDK enum/union/fixed-char DTOs stay here.
// constants: version, custom_size, first_angle, third_angle, object_view_type,
// model_view_type, drawing_member_type, imported_member_subtype.
OE_NX_EXPORT int32_t oe_nx_drafting_abi(int32_t constants[8],uint32_t count) noexcept;
OE_NX_EXPORT int32_t oe_nx_create_drafting_sheet(const char *name,int32_t units,
    double width,double height,double scale,int32_t projection,uint32_t *drawing,
    int32_t *query_code,char *error,uint32_t error_capacity) noexcept;
// codes: ask original drawing info, set final drawing info. No save occurs here.
OE_NX_EXPORT int32_t oe_nx_set_drafting_sheet_info_v1(uint32_t drawing,int32_t units,
    double width,double height,double scale,int32_t projection,int32_t codes[2],
    char *error,uint32_t capacity) noexcept;
OE_NX_EXPORT int32_t oe_nx_import_drafting_view(uint32_t drawing,uint32_t model_view,
    const char *model_name,double scale,const double reference[2],uint32_t *view,
    int32_t *query_code,char *error,uint32_t error_capacity) noexcept;
// v1 layout helper and anchored import preserve the original import entry.
// codes: scalarX, scalarY, scalarZ, smart point creation, helper visibility.
OE_NX_EXPORT int32_t oe_nx_create_layout_anchor_v1(uint32_t object_in_part,
    const double coordinates[3],uint32_t scalar_tags[3],uint32_t *anchor,
    int32_t codes[5],char *error,uint32_t capacity) noexcept;
OE_NX_EXPORT int32_t oe_nx_import_drafting_view_with_anchor_v1(uint32_t drawing,
    uint32_t model_view,const char *model_name,double scale,const double reference[2],
    uint32_t anchor,uint32_t *view,int32_t *query_code,char *error,uint32_t capacity) noexcept;
// raw: query_code, units, size_state, projection_angle; values: width,height,scale.
// Standard sheet sizes retain their raw kind and unknown width/height here.
OE_NX_EXPORT int32_t oe_nx_drafting_sheet_info(uint32_t drawing,int32_t raw[4],
    double values[3],char *error,uint32_t error_capacity) noexcept;
// Extension2 keeps the original extension intact. raw adds standard code and
// copied enum family (0 custom, 1 metric, 2 english, -1 unsupported).
OE_NX_EXPORT int32_t oe_nx_drafting_sheet_info_v2(uint32_t drawing,int32_t raw[6],
    double values[3],char *error,uint32_t error_capacity) noexcept;
#define OE_NX_BOUNDARY_VERSION 1u
#define OE_NX_STATUS_LIMIT 32u
#define OE_NX_TEXT_LIMIT 1024u
struct OeNxAssemAdd {
    uint32_t version, structure_bytes, parent, prototype_bytes, refset_bytes, name_bytes;
    int32_t layer;
    uint32_t reserved;
    const char *prototype, *refset, *name;
    double origin[3], csys[6];
};
struct OeNxLoadItem { int32_t code; char name[OE_NX_TEXT_LIMIT], description[OE_NX_TEXT_LIMIT]; };
struct OeNxOpenAddObservation {
    uint32_t version, structure_bytes, call_entered, call_returned, component, status_present;
    int32_t unloaded_count;
    uint32_t status_copied, status_released, complete;
    int32_t exception_code;
    uint32_t phase;
    char reference_set[64];
    OeNxLoadItem items[OE_NX_STATUS_LIMIT];
};
// Independent entry for loaded prototypes and complete orientation9. The
// original identity6/controlled-refset entry and UF diagnostics are preserved.
struct OeNxEntireLoadedAdd {
    uint32_t version,structure_bytes,parent,child,prototype_bytes,name_bytes;
    int32_t layer;
    uint32_t reserved;
    const char *prototype,*name;
    double origin[3],column_rotation[9];
};
struct OeNxEntireLoadedObservation {
    OeNxOpenAddObservation load;
    char vendor_default_refset[64],entire_refset[64];
    uint32_t direct_owner_matches,entire_set_returned,builder_destroyed,reserved;
};
OE_NX_EXPORT int32_t oe_nxopen_entire_loaded_add_v1(const OeNxEntireLoadedAdd *request,uint32_t request_bytes,
    OeNxEntireLoadedObservation *out,uint32_t out_bytes,char *error,uint32_t capacity) noexcept;
struct OeNxRefsetObservation {
    uint32_t version, structure_bytes, operation, owner, requested_body, reference_set;
    uint32_t found, initial_count, member_count, total_sets, complete, mutation_started;
    uint32_t members[OE_NX_STATUS_LIMIT];
    char name[64];
};
#define OE_NX_JT_VERSION 1u
// Borrowed NXOpen objects never cross the ABI. Bool is copied to uint32 only
// after normal return; on exception output pointers are not read or deleted.
struct OeNxBodyJtObservation {
    uint32_t version, structure_bytes, owner, solid, solid_owner;
    uint32_t call_entered, call_returned, has_jt, jt, up_to_date, complete;
    int32_t exception_code;
};
OE_NX_EXPORT int32_t oe_nx_body_jt(uint32_t owner,uint32_t solid,
    OeNxBodyJtObservation *observation,uint32_t observation_bytes,char *error,uint32_t error_capacity) noexcept;
// mode1 creates a new explicitly named set in this task's prototype; mode2 only queries.
OE_NX_EXPORT int32_t oe_nx_controlled_refset(uint32_t owner,uint32_t body,uint32_t mode,
    OeNxRefsetObservation *observation,uint32_t observation_bytes,char *error,uint32_t error_capacity) noexcept;
// Exactly one NXOpen AddComponent. Vendor objects stay in C++; only normal-return
// PartLoadStatus is queried and deleted, including later marshalling failure.
// Exception out pointers are unconfirmed and are neither queried nor deleted.
OE_NX_EXPORT int32_t oe_nxopen_add_component(const OeNxAssemAdd *options,uint32_t options_bytes,
    OeNxOpenAddObservation *observation,uint32_t observation_bytes,char *error,uint32_t error_capacity) noexcept;

OE_NX_EXPORT int32_t oe_nx_ask_object_state_v1(uint32_t object, int32_t *status, int32_t states[4]) noexcept;
OE_NX_EXPORT int32_t oe_nx_make_layout_anchor_visible_v1(uint32_t object) noexcept;
OE_NX_EXPORT int32_t oe_nx_ask_layout_visibility_v1(uint32_t object,int32_t codes[2],int32_t values[8]) noexcept;

// Independent assembly extension: no base ABI layout or old entry is changed.
// values: version, sizeof(load-state enum), installed fully-loaded value.
OE_NX_EXPORT int32_t oe_nx_assembly_abi_v1(int32_t values[3],uint32_t count) noexcept;
OE_NX_EXPORT int32_t oe_nx_step_assembly_import_v1(const OeNxStepImport *options,
    uint32_t *flatten_after_set,char *error,uint32_t capacity) noexcept;

// Fixed entire-part UF call. Raw vendor storage is copied with memcpy; invalid
// logical object representations are never evaluated as C++ bool here.
struct OeNxUfEntireAdd {
    uint32_t version, structure_bytes, parent, prototype_bytes, name_bytes;
    int32_t layer;
    const char *prototype, *name;
    double origin[3], csys[9];
};
struct OeNxUfEntireAddCall {
    uint32_t version, structure_bytes, status_bytes, tag_bytes, logical_bytes, returned;
    int32_t native_code;
    uint32_t instance;
    uint8_t before[24], after[24];
};
OE_NX_EXPORT int32_t oe_nx_uf_entire_add_v1(const OeNxUfEntireAdd *request,
    void *status,uint32_t status_bytes,OeNxUfEntireAddCall *call,uint32_t call_bytes,
    char *error,uint32_t capacity) noexcept;
// values: version, component-load method, components to load, partial loading,
// lightweight representations, generate missing family members. Getters only.
OE_NX_EXPORT int32_t oe_nx_assembly_load_diagnostic_v1(int32_t values[6],uint32_t count,
    char *error,uint32_t capacity) noexcept;

OE_NX_EXPORT int32_t oe_nx_close_control_part_v1(uint32_t part) noexcept;

// Explicit optional-NULL mechanism; with_anchor_v1 retains its nonzero guard.
OE_NX_EXPORT int32_t oe_nx_import_drafting_view_without_anchor_v1(uint32_t drawing,uint32_t model_view,
    const char *model_name,double scale,const double reference[2],uint32_t *view,int32_t *query_code,
    char *error,uint32_t capacity) noexcept;

// Drawing coordinates: installed UF_DRF contract states drawing WCS = absolute.
OE_NX_EXPORT int32_t oe_nx_sheet_parallel_reference_v1(uint32_t edge1,uint32_t edge2,uint32_t view,
    const double drawing_origin[3],uint32_t *dimension,int32_t *native_code,char *error,uint32_t error_capacity) noexcept;

// Copied current occurrence override observations; SDK objects never cross ABI.
OE_NX_EXPORT int32_t oe_nx_position_override_v1(uint32_t component,int32_t *raw_type,
    uint32_t *parent,char *error,uint32_t capacity) noexcept;

OE_NX_EXPORT int32_t oe_nx_reposition_occurrence_v1(uint32_t component,const double delta[16],int32_t *native_code) noexcept;

// ABI /1: ten int32 observations, no C++ state crosses this boundary.
OE_NX_EXPORT int32_t oe_nx_drawing_pdf_v1(uint32_t part, uint32_t sheet, const char *output, int32_t *observations, uint32_t count, char *error, uint32_t capacity);

// Explicit ephemeral annotation/WCS ABI; Rust owns scope and restoration acceptance.
OE_NX_EXPORT int32_t oe_nx_dimension_label_v1(uint32_t part,uint32_t dimension,const char *label,int32_t *observations,uint32_t count,char *below,uint32_t below_capacity,char *error,uint32_t error_capacity);
OE_NX_EXPORT int32_t oe_nx_drawing_wcs_v1(uint32_t part,int32_t requested,int32_t *observations,uint32_t count,char *error,uint32_t error_capacity);

OE_NX_EXPORT int32_t oe_nx_dimension_label_v2(uint32_t part,uint32_t dimension,const char *label,int32_t *observations,uint32_t count,char *below,uint32_t below_capacity,char *error,uint32_t error_capacity);
OE_NX_EXPORT int32_t oe_nx_dimension_label_update_v1(uint32_t part,uint32_t dimension,const char *label,int32_t *observations,uint32_t count,char *below,uint32_t below_capacity,char *error,uint32_t error_capacity,int32_t *update,uint32_t update_count,int32_t *error_codes,uint32_t error_codes_capacity);
OE_NX_EXPORT int32_t oe_nx_dimension_layout_v1(uint32_t part,uint32_t dimension,const double *origin,int32_t centered,int32_t *observations,uint32_t count,double *actual_origin,char *error,uint32_t error_capacity);
OE_NX_EXPORT int32_t oe_nx_drawing_csys_v1(uint32_t *wcs,int32_t *kind) noexcept;
OE_NX_EXPORT int32_t oe_nx_drawing_object_v1(uint32_t part,uint32_t tag,int32_t requested,int32_t *observations,uint32_t count,uint32_t *tags,double *coordinates,char *error,uint32_t error_capacity);

OE_NX_EXPORT int32_t oe_nx_restore_drawing_wcs_v1(uint32_t part,uint32_t original,int32_t *observations,uint32_t count,uint32_t *tags,char *error,uint32_t capacity);

OE_NX_EXPORT int32_t oe_nx_sheet_diameter_v1(uint32_t edge,uint32_t view,const double drawing_origin[3],uint32_t *dimension,int32_t *native_code,char *error,uint32_t error_capacity) noexcept;
