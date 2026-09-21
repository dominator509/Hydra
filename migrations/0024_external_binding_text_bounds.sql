ALTER TABLE external_tenant_binding
    ADD CONSTRAINT external_tenant_binding_text_bounds CHECK (
        char_length(provider) BETWEEN 1 AND 512
        AND btrim(provider) <> ''
        AND provider !~ '[[:cntrl:]]'
        AND char_length(external_tenant_id) BETWEEN 1 AND 512
        AND btrim(external_tenant_id) <> ''
        AND external_tenant_id !~ '[[:cntrl:]]'
        AND char_length(external_business_id) BETWEEN 1 AND 512
        AND btrim(external_business_id) <> ''
        AND external_business_id !~ '[[:cntrl:]]'
    );

-- revert: ALTER TABLE external_tenant_binding DROP CONSTRAINT external_tenant_binding_text_bounds;
