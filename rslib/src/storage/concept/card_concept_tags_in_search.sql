SELECT cc.card_id,
  c.tag,
  c.section
FROM card_concepts cc
  JOIN concepts c ON c.id = cc.concept_id
WHERE cc.card_id IN (
    SELECT cid
    FROM search_cids
  )