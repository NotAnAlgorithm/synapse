-- Synapse service — re-canonicalize corpus concept tags to the AAMC spine.
--
-- Concept identity is now aligned to the AAMC spine. The canonical concept tag
-- has FOUR segments: concept::<section>::<category>::<topic>
--   section  = AAMC section code (BB / CP / PS)
--   category = content-category code (1A, 4C, 7C)  == the aamc_category column
--   topic    = lowercase/underscore slug
--
-- This forward migration re-keys the four ad-hoc concept tags the seed corpus
-- shipped with to their canonical spine ids, and updates aamc_category to the
-- canonical 3rd segment where the category code changed (physics 4B -> 4C,
-- psych 7A -> 7C). biochem rows keep aamc_category '1A'.
--
--   concept::biochem::amino_acid_charge     -> concept::BB::1A::amino_acids
--   concept::biochem::enzyme_kinetics       -> concept::BB::1A::control_of_enzyme_activity
--   concept::physics::circuits_ohms_law     -> concept::CP::4C::circuit_elements
--   concept::psych::operant_conditioning    -> concept::PS::7C::associative_learning
--
-- The corpus stays SERVER-ONLY reference data: it is never shipped to the
-- device. We only re-key its tags so they reference the canonical spine ids.
-- concept_tags is a text[]; array_replace() swaps each old tag in place,
-- preserving any other tags a row may carry (records may have >1 concept_tag).

update public.corpus_chunks
set concept_tags = array_replace(
        concept_tags,
        'concept::biochem::amino_acid_charge',
        'concept::BB::1A::amino_acids'
    )
where concept_tags @> array['concept::biochem::amino_acid_charge'];

update public.corpus_chunks
set concept_tags = array_replace(
        concept_tags,
        'concept::biochem::enzyme_kinetics',
        'concept::BB::1A::control_of_enzyme_activity'
    )
where concept_tags @> array['concept::biochem::enzyme_kinetics'];

update public.corpus_chunks
set concept_tags = array_replace(
        concept_tags,
        'concept::physics::circuits_ohms_law',
        'concept::CP::4C::circuit_elements'
    ),
    aamc_category = '4C'
where concept_tags @> array['concept::physics::circuits_ohms_law'];

update public.corpus_chunks
set concept_tags = array_replace(
        concept_tags,
        'concept::psych::operant_conditioning',
        'concept::PS::7C::associative_learning'
    ),
    aamc_category = '7C'
where concept_tags @> array['concept::psych::operant_conditioning'];
