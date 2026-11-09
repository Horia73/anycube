# Plan: solver 4x4x4 cu soluții mai scurte decât TPR

Actualizat: 2026-11-10. Țintă: medie de 35–40 de mutări (OBTM), sub 10 s pe cub pe M1 Pro.
Cifrele măsurate sunt în `bench/RESULTS.md`.

## Unde suntem

| Etapă | Stare | Medie |
|---|---|---|
| M0. Măsurătoare TPR pe setul de 1.000 | făcut | 44,57 |
| M1. Aceleași faze în Rust, mod rapid | făcut, cu 0,26 peste TPR | 44,84 |
| M2/M3. Multe reduceri comparate după lungimea totală | făcut | 42,92 |
| Portofoliu mai mare în faza 2, fazele 2–3 în paralel | făcut | vezi `bench/RESULTS.md` |
| Solver 3x3x3 exact | făcut și validat, oprit implicit | fără câștig la cost rezonabil |

## Ce am învățat

1. **Reducerea rămâne la ~25 de mutări orice aș face în structura actuală.**
   Cu de 10 ori mai multe capete de fază 2, faza 3 scade cu ~0,4, dar faza 2
   crește cu ~0,3. Schimbul e aproape 1:1.
2. **Etapa 3x3x3 e aproape de limită.** De 10 ori mai mult efort two-phase
   aduce 0,2 mutări; căutarea exactă cu 400 de milioane de noduri aduce 0,3.
   Niciuna nu merită la bugetul de 10 s.
3. **Loturile de reduceri sunt mici.** De regulă 20–1.500 de cuburi reduse
   distincte, nu mii, iar statistica „cel mai bun din N" cere mii.
4. **Slack-ul e în reducere, nu în 3x3x3.** După numărul de stări, o reducere
   optimă ar avea ~19–20 de mutări; noi facem ~25. La 3x3x3 suntem la ~17
   față de ~15–16 cât ar permite lotul.

Concluzie: cu fazele actuale ne oprim pe la 42. Pentru 40 trebuie schimbată
structura reducerii.

## Ce urmează, în ordinea câștigului estimat

Estimările sunt presupuneri, nu măsurători.

**A. Fazele 1 și 2 unite într-una singură (−1 până la −1,5).**
Acum fac împreună ~13,3 mutări; rezolvate împreună, optim, ar face ~11,5–12.
Cere un tabel exact pentru toate centrele (~9,5 miliarde de stări, 2–5 GB) și
o coordonată reală pentru muchii în locul filtrării de acum. E pasul cel mai
mare și cel mai riscant.

**B. Faza 2 cu pruning pe muchii (necesar pentru A).**
Acum faza 2 generează soluții de centre și păstrează 1 din ~1.320, cele cu
muchii împerecheabile. O coordonată de muchii ar înlocui filtrarea cu căutare.

**C. Reduceri alese și după distanța la subgrupul domino (−0,3 până la −0,5).**
Faza 3 poate prefera reducerile din care 3x3x3 pornește cu o fază 1 scurtă.
Tabelul exact de 554 MB există deja și se aplică și în timpul fazei 3.

**D. Tabel 3x3x3 redus prin simetrie, de ~35 MB (−0,3 până la −0,5).**
Căutarea exactă de acum pierde timpul în accesări de memorie împrăștiate
într-un tabel de 554 MB. Unul care încape în cache ar fi de ~10 ori mai rapid.

**E. Limită de timp și interfață „anytime".**
Acum efortul se dă în număr de candidați, nu în secunde, deci timpul maxim
variază de la cub la cub.

## Riscuri

- A poate da mai puțin decât estimez; nimeni nu a publicat media pentru fazele
  1+2 unite, doar maximul (18, neverificat independent).
- Sub 40 nu pot promite. 35 e aproape de limita inferioară teoretică.

## Surse

- TPR: https://github.com/cs0x7f/TPR-4x4x4-Solver
- dwalton76: https://github.com/dwalton76/rubiks-cube-NxNxN-solver
- Limita 54 pentru fazele 1+2+3+3x3x3 (neverificată): https://github.com/AnanasClassic/4x4x4-obtm-54-research
