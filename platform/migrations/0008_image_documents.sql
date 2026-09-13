-- Photographs and scans as documents.
--
-- A delivery note photographed on a building site, a signed quotation run
-- through a scanner: neither carries any text a parser can read, and until now
-- the upload endpoint could not even name them. The worker now sends them to
-- the OCR sidecar, so `documents.source_type` needs a value for them.
--
-- One value, `image`, rather than one per format. The parser is the same
-- whatever the container, and the original bytes keep their own type in
-- `mime_type` and in object storage.
--
-- The constraint was declared inline in 0001, so it carries the name
-- PostgreSQL gives such constraints. Dropped and recreated rather than
-- altered: a CHECK cannot be modified in place.

ALTER TABLE documents DROP CONSTRAINT documents_source_type_check;
ALTER TABLE documents ADD CONSTRAINT documents_source_type_check
  CHECK (source_type IN ('pdf','docx','txt','md','html','url','json','csv','text','image'));
