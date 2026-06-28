CREATE OR REPLACE FUNCTION notify_material_change()
RETURNS trigger AS $$
DECLARE
  mat_no_val TEXT;
BEGIN
  IF TG_OP = 'DELETE' THEN
    mat_no_val := OLD.mat_no;
  ELSE
    mat_no_val := NEW.mat_no;
  END IF;

  PERFORM pg_notify(
    'materials_channel',
    json_build_object(
      'meta', json_build_object('event_type', 'material'),
      'data', json_build_object('mat_no', mat_no_val)
    )::text
  );

  IF TG_OP = 'DELETE' THEN
    RETURN OLD;
  END IF;
  RETURN NEW;
END;
$$ LANGUAGE plpgsql;

DROP TRIGGER IF EXISTS material_change_trigger ON materials;
CREATE TRIGGER material_change_trigger
AFTER INSERT OR UPDATE OR DELETE ON materials
FOR EACH ROW EXECUTE FUNCTION notify_material_change();
