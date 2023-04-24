;; Placeholder for the updated clarity-bitcoin lib

;; It should return (ok wtxid) if it was mined
(define-public (was-segwit-tx-mined-compact
	(burn-height uint) ;; bitcoin block height
	(tx (buff 4096)) ;; tx to check
	(header (buff 80)) ;; bitcoin block header
	(tx-index uint)
	(tree-depth uint)
	(wproof (list 14 (buff 32))) ;; merkle proof for wtxids
	(ctx (buff 1024)) ;; coinbase tx, contains the witness root hash
	(cproof (list 14 (buff 32))) ;; merkle proof for coinbase tx
	)
	(if (> (len tx) u0)
		(ok 0x0011223344556677889900112233445566778899001122334455667788990011)
		(err u1)
	)
)
