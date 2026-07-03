DROP INDEX IF EXISTS idx_card_concepts_concept;
DROP TABLE IF EXISTS card_concepts;
DROP TABLE IF EXISTS concepts;
UPDATE col
SET ver = 18;