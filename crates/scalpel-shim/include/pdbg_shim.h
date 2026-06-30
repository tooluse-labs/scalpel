#ifndef PDBG_SHIM_H
#define PDBG_SHIM_H

#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

typedef enum pdbg_status {
    PDBG_OK = 0,
    PDBG_ERROR_GENERIC = 1,
    PDBG_ERROR_PASSWORD = 2,
    PDBG_ERROR_FORMAT = 3,
    PDBG_ERROR_UNSUPPORTED = 4,
    PDBG_ERROR_CANCELLED = 5,
    PDBG_ERROR_OOM = 6,
    PDBG_ERROR_SECURITY = 7,
    PDBG_ERROR_LIMIT = 8
} pdbg_status;

typedef struct pdbg_error {
    pdbg_status status;
    int mupdf_code;
    char message[1024];
} pdbg_error;

typedef pdbg_status (*pdbg_test_callback)(void *user, pdbg_error *err);

typedef struct pdbg_context pdbg_context;
typedef struct pdbg_doc pdbg_doc;
typedef struct pdbg_buffer pdbg_buffer;
typedef struct pdbg_image pdbg_image;
typedef struct pdbg_node_list pdbg_node_list;
typedef struct pdbg_diagnostic_list pdbg_diagnostic_list;
typedef struct pdbg_text_page pdbg_text_page;
typedef struct pdbg_visual_page pdbg_visual_page;
typedef struct pdbg_cancel_token pdbg_cancel_token;
typedef struct pdbg_xref_table pdbg_xref_table;

typedef struct pdbg_object_id {
    int num;
    int gen;
} pdbg_object_id;

typedef enum pdbg_xref_entry_kind {
    PDBG_XREF_ENTRY_FREE = 0,
    PDBG_XREF_ENTRY_NORMAL = 1,
    PDBG_XREF_ENTRY_COMPRESSED = 2,
} pdbg_xref_entry_kind;

typedef struct pdbg_xref_entry_info {
    int num;
    int gen;          /* object generation (0 for compressed entries) */
    int kind;         /* pdbg_xref_entry_kind */
    uint64_t offset;  /* byte offset for normal entries; containing object
                         stream number for compressed; raw value for free */
    int objstm_index; /* index inside the object stream for compressed
                         entries, -1 otherwise */
    int section;      /* xref section the entry resolves from, newest = 0;
                         -1 when the entry is undefined in every section */
} pdbg_xref_entry_info;

typedef enum pdbg_node_kind {
    PDBG_NODE_DOCUMENT_ROOT = 0,
    PDBG_NODE_TRAILER = 1,
    PDBG_NODE_CATALOG = 2,
    PDBG_NODE_XREF_ROOT = 3,
    PDBG_NODE_XREF_OBJECT = 4,
    PDBG_NODE_PAGE_ROOT = 5,
    PDBG_NODE_PAGE = 6,
    PDBG_NODE_PATH_TOKEN = 7,
    PDBG_NODE_INDIRECT_REF = 8,
    PDBG_NODE_STREAM = 9,
    PDBG_NODE_RESOURCE_GROUP = 10
} pdbg_node_kind;

typedef enum pdbg_resource_group {
    PDBG_RESOURCE_FONTS = 0,
    PDBG_RESOURCE_IMAGES = 1,
    PDBG_RESOURCE_XOBJECTS = 2,
    PDBG_RESOURCE_COLOR_SPACES = 3,
    PDBG_RESOURCE_PATTERNS = 4,
    PDBG_RESOURCE_SHADINGS = 5,
    PDBG_RESOURCE_ANNOTATIONS = 6,
    PDBG_RESOURCE_WIDGETS = 7
} pdbg_resource_group;

typedef struct pdbg_node_id {
    uint64_t document_id;
    pdbg_node_kind kind;
    pdbg_object_id object;
    int has_object;
    uint32_t page_index;
    uint64_t path_token;
    int decoded;
    pdbg_resource_group resource_group;
} pdbg_node_id;

typedef struct pdbg_string_pair {
    char *key;
    char *value;
} pdbg_string_pair;

typedef struct pdbg_permissions {
    int print;
    int modify;
    int copy;
    int annotate;
    int fill_forms;
    int extract_accessibility;
    int assemble;
    int high_quality_print;
} pdbg_permissions;

typedef enum pdbg_object_kind {
    PDBG_OBJECT_NULL = 0,
    PDBG_OBJECT_BOOL = 1,
    PDBG_OBJECT_INT = 2,
    PDBG_OBJECT_REAL = 3,
    PDBG_OBJECT_NAME = 4,
    PDBG_OBJECT_STRING = 5,
    PDBG_OBJECT_ARRAY = 6,
    PDBG_OBJECT_DICT = 7,
    PDBG_OBJECT_INDIRECT_REF = 8,
    PDBG_OBJECT_STREAM = 9,
    PDBG_OBJECT_PAGE = 10,
    PDBG_OBJECT_XREF_ENTRY = 11,
    PDBG_OBJECT_TRAILER = 12,
    PDBG_OBJECT_METADATA = 13,
    PDBG_OBJECT_UNKNOWN = 14
} pdbg_object_kind;

typedef enum pdbg_object_value_kind {
    PDBG_VALUE_NULL = 0,
    PDBG_VALUE_BOOL = 1,
    PDBG_VALUE_INT = 2,
    PDBG_VALUE_REAL = 3,
    PDBG_VALUE_NAME = 4,
    PDBG_VALUE_STRING_BYTES = 5,
    PDBG_VALUE_INDIRECT_REF = 6,
    PDBG_VALUE_CONTAINER = 7,
    PDBG_VALUE_UNKNOWN = 8
} pdbg_object_value_kind;

typedef enum pdbg_pdf_string_kind {
    PDBG_STRING_LITERAL = 0,
    PDBG_STRING_HEX = 1,
    PDBG_STRING_UNKNOWN = 2
} pdbg_pdf_string_kind;

typedef enum pdbg_diagnostic_severity {
    PDBG_DIAG_INFO = 0,
    PDBG_DIAG_WARNING = 1,
    PDBG_DIAG_ERROR = 2
} pdbg_diagnostic_severity;

typedef enum pdbg_diagnostic_code {
    PDBG_DIAG_MISSING_OBJECT = 0,
    PDBG_DIAG_BROKEN_XREF_ENTRY = 1,
    PDBG_DIAG_STREAM_DECODE_FAILURE = 2,
    PDBG_DIAG_ENCRYPTION_PASSWORD_FAILURE = 3,
    PDBG_DIAG_REPAIR_WARNING = 4,
    PDBG_DIAG_JAVASCRIPT_DISABLED = 5,
    PDBG_DIAG_EMBEDDED_FILE_DETECTED = 6,
    PDBG_DIAG_EXTERNAL_REFERENCE_DETECTED = 7,
    PDBG_DIAG_RESOURCE_MISSING = 8,
    PDBG_DIAG_RENDER_WARNING = 9,
    PDBG_DIAG_UNKNOWN = 10
} pdbg_diagnostic_code;

typedef struct pdbg_diagnostic {
    pdbg_diagnostic_severity severity;
    pdbg_diagnostic_code code;
    char *message;
    pdbg_node_id node;
    int has_node;
    uint32_t page_index;
    int has_page_index;
    pdbg_object_id object;
    int has_object;
} pdbg_diagnostic;

typedef struct pdbg_object_value {
    pdbg_object_value_kind kind;
    int bool_value;
    int64_t int_value;
    double real_value;
    char *name_value;
    uint8_t *bytes;
    size_t byte_len;
    pdbg_pdf_string_kind string_kind;
    int is_text_string;
    char *decoded_text;
    pdbg_object_id ref_value;
} pdbg_object_value;

typedef struct pdbg_dict_entry {
    char *key;
    pdbg_node_id node;
    pdbg_object_kind object_kind;
    pdbg_object_id object;
    int has_object;
    char *label;
    char *preview;
    int has_children;
    int has_stream;
    size_t child_count;
    int has_child_count;
    uint64_t byte_size_hint;
    int has_byte_size_hint;
    pdbg_diagnostic_severity max_diagnostic_severity;
    size_t diagnostic_count;
    pdbg_diagnostic_list *diagnostics;
} pdbg_dict_entry;

typedef struct pdbg_text_span {
    char *text;
    size_t text_len;
    float x;
    float y;
    float width;
    float height;
    uint32_t page_index;
    int untrusted;
} pdbg_text_span;

typedef enum pdbg_visual_kind {
    PDBG_VISUAL_TEXT = 0,
    PDBG_VISUAL_IMAGE = 1,
    PDBG_VISUAL_VECTOR = 2,
    PDBG_VISUAL_GRID = 3,
    PDBG_VISUAL_ANNOTATION = 4,
    PDBG_VISUAL_WIDGET = 5,
    PDBG_VISUAL_UNKNOWN = 255
} pdbg_visual_kind;

#define PDBG_VISUAL_OBJECT_TYPE_LEN 64
#define PDBG_VISUAL_OBJECT_DATA_LEN 256

typedef struct pdbg_visual_options {
    int include_text;
    int include_images;
    int include_vectors;
    size_t max_elements;
} pdbg_visual_options;

typedef struct pdbg_visual_element {
    pdbg_visual_kind kind;
    float x;
    float y;
    float width;
    float height;
    uint32_t page_index;
    pdbg_object_id object;
    int has_object;
    int untrusted;
    char object_type[PDBG_VISUAL_OBJECT_TYPE_LEN];
    char object_data[PDBG_VISUAL_OBJECT_DATA_LEN];
} pdbg_visual_element;

typedef enum pdbg_repair_policy {
    PDBG_REPAIR_DEFAULT = 0,
    PDBG_REPAIR_NEVER = 1,
    PDBG_REPAIR_ALLOW = 2
} pdbg_repair_policy;

typedef enum pdbg_color_mode {
    PDBG_COLOR_RGBA = 0,
    PDBG_COLOR_GRAYSCALE = 1,
    PDBG_COLOR_INVERTED = 2
} pdbg_color_mode;

typedef struct pdbg_stream_summary {
    pdbg_object_id object;
    char **filters;
    size_t filter_count;
    uint64_t raw_size_hint;
    int has_raw_size_hint;
    uint64_t decoded_size_hint;
    int has_decoded_size_hint;
    int can_decode;
    int image_preview_available;
} pdbg_stream_summary;

typedef struct pdbg_open_options {
    int safe_mode;
    int disable_javascript;
    int enable_ocr;
    uint64_t max_store_bytes;
    uint64_t max_decoded_stream_bytes;
    uint32_t max_filter_expansion_ratio;
    uint32_t max_object_depth;
    pdbg_repair_policy repair_policy;
    int allow_external_references;
} pdbg_open_options;

typedef struct pdbg_render_options {
    float zoom;
    int rotation_degrees;
    uint32_t max_width;
    uint32_t max_height;
    uint64_t max_pixels;
    uint64_t max_output_bytes;
    pdbg_color_mode color_mode;
    uint64_t layer_config_token;
} pdbg_render_options;

typedef struct pdbg_text_options {
    int sort_by_position;
    int include_coordinates;
    size_t max_chars;
    size_t max_blocks;
} pdbg_text_options;

typedef struct pdbg_document_summary_out {
    uint64_t document_id;
    char *file_path;
    char *file_hash;
    char *pdf_version;
    size_t page_count;
    size_t xref_size;
    size_t parsed_object_count;
    int has_parsed_object_count;
    int encrypted;
    int needs_password;
    pdbg_permissions permissions;
    pdbg_string_pair *metadata;
    size_t metadata_len;
    int safe_mode;
    int javascript_disabled;
    int repaired_or_damaged;
    int embedded_files_detected;
    int external_references_detected;
    int ocr_enabled;
    pdbg_diagnostic_list *diagnostics;
} pdbg_document_summary_out;

typedef struct pdbg_object_detail_out {
    pdbg_node_id id;
    pdbg_object_id object;
    int has_object;
    pdbg_object_kind kind;
    char *label;
    char *preview;
    pdbg_object_value value;
    pdbg_node_list *children;
    pdbg_node_list *dictionary_entries;
    int has_stream;
    pdbg_stream_summary stream;
    pdbg_diagnostic_list *diagnostics;
} pdbg_object_detail_out;

pdbg_status pdbg_context_new(pdbg_context **out, pdbg_error *err);
void pdbg_context_drop(pdbg_context *ctx);

pdbg_status pdbg_cancel_token_new(pdbg_cancel_token **out, pdbg_error *err);
void pdbg_cancel_token_cancel(pdbg_cancel_token *token);
void pdbg_cancel_token_drop(pdbg_cancel_token *token);

pdbg_status pdbg_document_open(
    pdbg_context *ctx,
    const char *path,
    const char *password,
    const pdbg_open_options *options,
    pdbg_doc **out,
    pdbg_error *err);

pdbg_status pdbg_document_open_fd(
    pdbg_context *ctx,
    int fd,
    const char *display_path,
    const char *password,
    const pdbg_open_options *options,
    pdbg_doc **out,
    pdbg_error *err);

void pdbg_document_drop(pdbg_doc *doc);

pdbg_status pdbg_document_summary(
    pdbg_doc *doc,
    pdbg_document_summary_out *out,
    pdbg_error *err);

pdbg_status pdbg_node_children(
    pdbg_doc *doc,
    const pdbg_node_id *node,
    size_t offset,
    size_t limit,
    pdbg_node_list **out,
    pdbg_error *err);

pdbg_status pdbg_object_detail(
    pdbg_doc *doc,
    const pdbg_node_id *node,
    pdbg_object_detail_out *out,
    pdbg_error *err);

pdbg_status pdbg_xref_table_load(
    pdbg_doc *doc,
    size_t offset,
    size_t limit,
    pdbg_xref_table **out,
    pdbg_error *err);
void pdbg_xref_table_drop(pdbg_xref_table *table);
size_t pdbg_xref_table_len(const pdbg_xref_table *table);
size_t pdbg_xref_table_total(const pdbg_xref_table *table);
size_t pdbg_xref_table_start(const pdbg_xref_table *table);
size_t pdbg_xref_table_sections(const pdbg_xref_table *table);
const pdbg_xref_entry_info *pdbg_xref_table_items(const pdbg_xref_table *table);

pdbg_status pdbg_stream_load(
    pdbg_doc *doc,
    pdbg_object_id object,
    int decoded,
    uint64_t offset,
    size_t limit,
    pdbg_cancel_token *cancel,
    pdbg_buffer **out,
    pdbg_error *err);

/* Decode an image XObject into RGBA pixels. `max_dimension` bounds the decoded
 * width/height (the image is downscaled when larger); `max_output_bytes`
 * bounds the RGBA buffer (0 selects the default). Decode warnings are
 * reported through the image diagnostics. */
pdbg_status pdbg_image_object_load(
    pdbg_doc *doc,
    pdbg_object_id object,
    uint32_t max_dimension,
    uint64_t max_output_bytes,
    pdbg_cancel_token *cancel,
    pdbg_image **out,
    pdbg_error *err);

/* Stream the full object contents (raw or decoded) to a file in one pass.
 * Stops at `max_bytes` when non-zero (reported via `capped`). Output goes to
 * a sibling temp file that is renamed into place on success, so errors and
 * cancellation never modify an existing destination. */
pdbg_status pdbg_stream_save(
    pdbg_doc *doc,
    pdbg_object_id object,
    int decoded,
    const char *path,
    uint64_t max_bytes,
    pdbg_cancel_token *cancel,
    uint64_t *bytes_written,
    int *capped,
    pdbg_error *err);

pdbg_status pdbg_page_render(
    pdbg_doc *doc,
    uint32_t page_index,
    const pdbg_render_options *options,
    pdbg_cancel_token *cancel,
    pdbg_image **out,
    pdbg_error *err);

pdbg_status pdbg_page_extract_text(
    pdbg_doc *doc,
    uint32_t page_index,
    const pdbg_text_options *options,
    pdbg_cancel_token *cancel,
    pdbg_text_page **out,
    pdbg_error *err);

pdbg_status pdbg_page_extract_visuals(
    pdbg_doc *doc,
    uint32_t page_index,
    const pdbg_visual_options *options,
    pdbg_cancel_token *cancel,
    pdbg_visual_page **out,
    pdbg_error *err);

void pdbg_buffer_drop(pdbg_buffer *buffer);
void pdbg_image_drop(pdbg_image *image);
void pdbg_node_list_drop(pdbg_node_list *list);
void pdbg_text_page_drop(pdbg_text_page *text);
void pdbg_visual_page_drop(pdbg_visual_page *visuals);
void pdbg_document_summary_out_drop(pdbg_document_summary_out *out);
void pdbg_object_detail_out_drop(pdbg_object_detail_out *out);

const uint8_t *pdbg_buffer_data(const pdbg_buffer *buffer);
size_t pdbg_buffer_len(const pdbg_buffer *buffer);
uint64_t pdbg_buffer_total_size_hint(const pdbg_buffer *buffer);
int pdbg_buffer_truncated(const pdbg_buffer *buffer);
size_t pdbg_buffer_diagnostic_count(const pdbg_buffer *buffer);
pdbg_status pdbg_buffer_diagnostic_get(
    const pdbg_buffer *buffer,
    size_t index,
    pdbg_diagnostic *out,
    pdbg_error *err);

uint32_t pdbg_image_width(const pdbg_image *image);
uint32_t pdbg_image_height(const pdbg_image *image);
size_t pdbg_image_stride(const pdbg_image *image);
const uint8_t *pdbg_image_rgba_pixels(const pdbg_image *image);
size_t pdbg_image_diagnostic_count(const pdbg_image *image);
pdbg_status pdbg_image_diagnostic_get(
    const pdbg_image *image,
    size_t index,
    pdbg_diagnostic *out,
    pdbg_error *err);

size_t pdbg_node_list_len(const pdbg_node_list *list);
int pdbg_node_list_has_total_count(const pdbg_node_list *list);
size_t pdbg_node_list_total_count(const pdbg_node_list *list);
pdbg_status pdbg_node_list_get(
    const pdbg_node_list *list,
    size_t index,
    pdbg_dict_entry *out,
    pdbg_error *err);

size_t pdbg_diagnostic_list_len(const pdbg_diagnostic_list *list);
pdbg_status pdbg_diagnostic_list_get(
    const pdbg_diagnostic_list *list,
    size_t index,
    pdbg_diagnostic *out,
    pdbg_error *err);

size_t pdbg_text_page_span_count(const pdbg_text_page *text);
pdbg_status pdbg_text_page_span_get(
    const pdbg_text_page *text,
    size_t index,
    pdbg_text_span *out,
    pdbg_error *err);

size_t pdbg_visual_page_element_count(const pdbg_visual_page *visuals);
pdbg_status pdbg_visual_page_element_get(
    const pdbg_visual_page *visuals,
    size_t index,
    pdbg_visual_element *out,
    pdbg_error *err);

pdbg_status pdbg_test_invoke_callback(
    pdbg_test_callback callback,
    void *user,
    pdbg_error *err);

int pdbg_test_document_owned_fd(const pdbg_doc *doc);
int pdbg_test_fd_is_open(int fd);

#ifdef __cplusplus
}
#endif

#endif
