# Plan: solver 4x4x4 cu soluții mai scurte decât TPR

Actualizat: 2026-11-10. Țintă: medie de 35–40 de mutări (OBTM), sub 10 s pe cub pe M1 Pro.
Cifrele măsurate sunt în `bench/RESULTS.md`.

## Unde suntem

| Pas | Stare | Medie |
|---|---|---|
| Măsurătoare TPR pe setul de 1.000 | făcut | 44,57 |
| Aceleași faze în Rust, mod rapid | făcut, cu 0,26 peste TPR | 44,84 |
| Multe reduceri comparate după lungimea totală | făcut | 42,92 |
| Fazele 1 și 2 unite, cu tabel exact pentru centre | făcut | ~41,0 |
| Mai multe capete, căutare 3x3x3 adâncită pe cele mai bune reduceri | făcut | 40,67 pe toate cele 1.000 |
| Limită de timp pe cub | făcut (`--time`, implicit 8 s) | |
| Solver 3x3x3 exact | făcut și validat, oprit implicit | fără câștig la cost rezonabil |
| Reduceri mai lungi, apropiate de subgrupul domino | făcut, oprit implicit | −0,07, în marja de zgomot |

## Ce am învățat

1. **Unirea fazelor 1 și 2 a meritat.** Primul capăt apare la ~11,1 mutări în
   loc de ~12,4, iar pe total câștigul e de ~0,8–1,2 mutări.
2. **Etapa 3x3x3 e aproape de limită.** De 10 ori mai mult efort two-phase
   aduce 0,2 mutări. Căutarea exactă costă ~1 s pentru o singură întrebare
   „există soluție în 15?" și e de ~2.000 de ori mai scumpă pe rezultat util
   decât two-phase. Un tabel de 10 ori mai rapid nu ar schimba concluzia.
3. **Fiecare buget suplimentar aduce tot mai puțin.** De la 3.000 la 10.000 de
   capete: −0,15. Încă un nivel de reduceri: −0,1 până la −0,2, la de 3 ori timpul.
4. **Lungimile pe etape acum:** faza unită 12,78, faza 3 10,90, 3x3x3 17,00.

Concluzie: cu structura actuală suntem pe la 40,6–40,8. Sub 40 cere încă o
schimbare de structură, nu mai mult buget.

## Ce ar mai putea aduce un câștig real

Estimările sunt presupuneri, nu măsurători.

**A. Faza 3 cu mai multe mutări permise.**
Faza 3 folosește doar 17 mutări și are ~11 mutări în medie. Dacă ținta fazei
unite ar fi mai slabă, iar faza 3 ar primi și sferturile de tură R/L, ar scădea
ambele. Coordonata de împerechere de acum nu mai e definită atunci, deci cere
o coordonată nouă și, probabil, un tabel mult mai mare.

**B. Pruning pe muchii în faza unită.**
Acum doar 1 din ~20.000 de secvențe care așază centrele trece și testele de
muchii. Un prag admisibil pe muchii ar permite un nivel în plus în același timp.

**C. Faza unită care ține cont de faza 3.**
Capetele se aleg acum după o estimare a fazei 3 care e cu ~1,5 mutări sub
realitate. O estimare mai strânsă ar alege capete mai bune.

## Riscuri

- Sub 40 nu pot promite. 35 e aproape de limita inferioară teoretică.
- Tabelele ocupă 3,2 GB pe disc, iar cel de centre se generează în ~2 minute.
- Limita de timp e „moale": fiecare etapă își așteaptă primul rezultat, deci
  un cub greu poate depăși limita.

## Surse

- TPR: https://github.com/cs0x7f/TPR-4x4x4-Solver
- dwalton76: https://github.com/dwalton76/rubiks-cube-NxNxN-solver
- Limita 54 pentru fazele 1+2+3+3x3x3 (neverificată): https://github.com/AnanasClassic/4x4x4-obtm-54-research
