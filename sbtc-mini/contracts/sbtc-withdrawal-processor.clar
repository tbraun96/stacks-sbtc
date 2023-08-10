(define-constant withdrawal-state-requested 0x00)
(define-constant withdrawal-state-fulfilled 0x01)
(define-constant withdrawal-state-reclaimed 0x02)

(define-constant sbtc-token-burnchain-lock-time u2100)

(define-constant err-token-lock-failed (err u5700))
(define-constant err-token-unlock-failed (err u5701))
(define-constant err-unknown-withdrawal-request (err u5702))
(define-constant err-withdrawal-not-epxired (err u5703))
(define-constant err-withdrawal-not-requested (err u5704))
(define-constant err-wrong-destination (err u5705))
(define-constant err-unacceptable-expiry-height (err u5706))
(define-constant err-wrong-value (err u5707))

(define-read-only (extract-request-data (tx (buff 4096)) (p2tr-unlock-script (buff 128)))
	;; It verifies the tapscript is the expected format.
	;; - "before burn height N, address X can spend, or else Y can spend"
	;; - check the expiry to make sure there is enough time to fulfil it
	;; - check if the script corresponds to the witness program in the tx

	;; Extract data from the Bitcoin transaction/tapscript:
	;; - The total BTC value requested to be pegged out, in sats
	;; - The principal pegging out
	;; - The burnchain withdrawal expiry height

	;; To retrieve the principal of the entity pegging out (sender):
	;; message = something like amount + recipient scriptPubkey + nonce
	;; signature = embedded somewhere in the tapscript
	;; (principal-of? (unwrap! (secp256k1-recover? message signature) err-recovery-failed))

	;; make the type checker happy
	(if true (ok {
		sender: 'ST000000000000000000002AMW42H,
		destination: { version: 0x00, hashbytes: 0x0011223344556699001122334455669900112233445566990011223344556699},
		value: u100,
		expiry-burn-height: (+ burn-block-height sbtc-token-burnchain-lock-time)
		})
		(err u999999)
		)
)

(define-public (register-withdrawal-request 
	(burn-height uint)
	(tx (buff 4096))
	(p2tr-unlock-script (buff 128))
	(header (buff 80))
	(tx-index uint)
	(tree-depth uint)
	(wproof (list 14 (buff 32)))
	(ctx (buff 1024))
	(cproof (list 14 (buff 32)))
	)
	(let (
		;; check if the tx was mined (todo: segwit wtxid)
		;; #[filter(tx)]
		(burn-wtxid (try! (contract-call? .clarity-bitcoin was-segwit-tx-mined-compact burn-height tx header tx-index tree-depth wproof 0x 0x ctx cproof)))
		;; get the peg out data
		;; #[filter(ts)]
		(withdrawal-data (try! (extract-request-data tx p2tr-unlock-script)))
		)
		;; There are still open questions about this part of the API.
		;; We can submit the P2TR funding transaction along with unlock script
		;; and store it, but it seems quite hard to verify that the unlock
		;; script can actually spend the P2TR output in Clarity. We have to
		;; derive the witness program and compare it with the one in the 
		;; transaction.

		;; check if the tx has not been processed before and if it
		;; reached the minimum amount of confirmations.
		(try! (contract-call? .sbtc-registry assert-new-burn-wtxid-and-height burn-wtxid burn-height))
		;; check that the expiry height is acceptable
		(asserts! (>= (get expiry-burn-height withdrawal-data) (+ burn-block-height sbtc-token-burnchain-lock-time)) err-unacceptable-expiry-height)
		;; lock sender's the tokens
		(unwrap! (contract-call? .sbtc-token protocol-lock (get value withdrawal-data) (get sender withdrawal-data)) err-token-lock-failed)
		;; insert the request, returns the withdrawal request-id
		(contract-call? .sbtc-registry insert-withdrawal-request (get value withdrawal-data) (get sender withdrawal-data) (get expiry-burn-height withdrawal-data) (get destination withdrawal-data) p2tr-unlock-script)
	)
)

(define-read-only (extract-fulfilment-data (tx (buff 4096)))
	;; Extract data from the Bitcoin transaction/tapscript:
	;; - The total BTC value that was paid out, in sats
	;; - The recipient

	(if true (ok {
		destination: { version: 0x00, hashbytes: 0x0011223344556699001122334455669900112233445566990011223344556699},
		value: u100
		})
		(err u999999)
		)
)

(define-public (relay-withdrawal-fulfilment
	(withdrawal-request-id uint)
	(burn-height uint)
	(tx (buff 4096))
	(header (buff 80))
	(tx-index uint)
	(tree-depth uint)
	(wproof (list 14 (buff 32)))
	(ctx (buff 1024))
	(cproof (list 14 (buff 32)))
	)
	(let (
		;; check if the tx was mined
		;; #[filter(tx)]
		(burn-wtxid (try! (contract-call? .clarity-bitcoin was-segwit-tx-mined-compact burn-height tx header tx-index tree-depth wproof 0x 0x ctx cproof)))
		;; get the fulfilment data
		;; #[filter(ts)]
		(fulfilment-data (try! (extract-fulfilment-data tx)))
		;; get the pending withdrawal and mark it as settled.
		;; the call will fail if the request is no longer pending.
		(withdrawal-request (try! (contract-call? .sbtc-registry get-and-settle-pending-withdrawal-request withdrawal-request-id withdrawal-state-fulfilled)))
		)
		;; we do not actually care who fulfilled the withdrawal request. Anyone
		;; can pay the btc, it does not have to come from the peg wallet.

		;; check if the tx has not been processed before and if it
		;; reached the minimum amount of confirmations.
		(try! (contract-call? .sbtc-registry assert-new-burn-wtxid-and-height burn-wtxid burn-height))
		;; check if the right destination address got paid
		(asserts! (is-eq (get destination fulfilment-data) (get destination withdrawal-request)) err-wrong-destination)
		;; check if the requested value was paid
		;; possible feature: allow transactions to partially peg out a request instead of
		;; all-or-nothing.
		(asserts! (>= (get value fulfilment-data) (get value withdrawal-request)) err-wrong-value)
		;; burn the locked user tokens
		(contract-call? .sbtc-token protocol-burn-locked (get value withdrawal-request) (get sender withdrawal-request))
	)
)

;; unlocks the sBTC tokens after expiry
(define-public (reclaim-locked-tokens (withdrawal-request-id uint))
	(let (
		;; get the pending withdrawal and mark it as settled.
		;; the call will fail if the request is no longer pending.
		(withdrawal-request (try! (contract-call? .sbtc-registry get-and-settle-pending-withdrawal-request withdrawal-request-id withdrawal-state-reclaimed)))
		)
		;; check if the withdrawal request has expired (pending check is done above)
		(asserts! (<= (get expiry-burn-height withdrawal-request) burn-block-height) err-withdrawal-not-epxired)
		;; unlock the locked user tokens
		(contract-call? .sbtc-token protocol-unlock (get value withdrawal-request) (get sender withdrawal-request))
	)
)
